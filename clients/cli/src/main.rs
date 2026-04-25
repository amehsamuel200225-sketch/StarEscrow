use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use hex;
// Chrono used via modules if needed


/// StarEscrow CLI — interact with the escrow contract on Stellar Testnet.
///
/// Prerequisites:
///   - Stellar CLI installed: https://developers.stellar.org/docs/tools/developer-tools/cli/install-cli
///   - Contract deployed and ESCROW_CONTRACT_ID set in env
///   - PAYER_SECRET and FREELANCER_SECRET set in env
#[derive(Parser)]
#[command(name = "star-escrow", version, about)]
struct Cli {
    /// Path to a TOML config file. Defaults to ~/.star-escrow/config.toml.
    /// Config values are overridden by explicit CLI flags.
    #[arg(long, value_name = "FILE")]
    config: Option<std::path::PathBuf>,

    /// Network shorthand: testnet, mainnet, or futurenet.
    /// Sets --rpc-url and --network-passphrase automatically.
    /// Cannot be combined with --rpc-url or --network-passphrase.
    #[arg(long, value_enum, conflicts_with_all = ["rpc_url", "network_passphrase"])]
    network: Option<Network>,

    /// Soroban RPC endpoint. Defaults to testnet if neither --network nor --rpc-url is given.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Network passphrase. Defaults to testnet if neither --network nor --network-passphrase is given.
    #[arg(long)]
    network_passphrase: Option<String>,

    /// Output results as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Simulate the transaction without submitting it to the network.
    /// Uses Soroban's simulation endpoint to preview the transaction
    /// outcome.
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Commands,
}

/// TOML config file format:
///
/// ```toml
/// rpc_url = "https://soroban-testnet.stellar.org"
/// network_passphrase = "Test SDF Network ; September 2015"
/// contract_id = "C..."
/// ```
#[derive(Debug, Default, Deserialize, Serialize)]
struct ConfigFile {
    rpc_url: Option<String>,
    network_passphrase: Option<String>,
    contract_id: Option<String>,
}

impl ConfigFile {
    fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing config file {}", path.display()))
    }

    fn load_default_or_explicit(explicit: Option<&std::path::Path>) -> Result<Self> {
        let path = match explicit {
            Some(p) => p.to_path_buf(),
            None => {
                let home = std::env::var("HOME").unwrap_or_default();
                std::path::PathBuf::from(home)
                    .join(".star-escrow")
                    .join("config.toml")
            },
        };
        if path.exists() {
            Self::load(&path)
        } else if explicit.is_some() {
            anyhow::bail!("config file not found: {}", path.display());
        } else {
            Ok(Self::default())
        }
    }
}

#[derive(clap::ValueEnum, Clone)]
enum Network {
    Testnet,
    Mainnet,
    Futurenet,
}

impl Network {
    fn rpc_url(&self) -> &'static str {
        match self {
            Network::Testnet => "https://soroban-testnet.stellar.org",
            Network::Mainnet => "https://soroban-mainnet.stellar.org",
            Network::Futurenet => "https://rpc-futurenet.stellar.org",
        }
    }

    fn passphrase(&self) -> &'static str {
        match self {
            Network::Testnet => "Test SDF Network ; September 2015",
            Network::Mainnet => "Public Global Stellar Network ; September 2015",
            Network::Futurenet => "Test SDF Future Network ; October 2022",
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise protocol config (admin, fee)
    Init {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "ADMIN_SECRET")]
        admin_secret: String,
        /// Fee in basis points (e.g. 100 = 1%)
        #[arg(long, default_value = "0")]
        fee_bps: u32,
        /// Fee collector Stellar address
        #[arg(long)]
        fee_collector: String,
    },
    /// Pause all state-changing operations (admin only)
    Pause {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "ADMIN_SECRET")]
        admin_secret: String,
    },
    /// Unpause the contract (admin only)
    Unpause {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "ADMIN_SECRET")]
        admin_secret: String,
    },
    /// Create a new escrow and lock funds
    Create {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "PAYER_SECRET")]
        payer_secret: String,
        #[arg(long)]
        freelancer: String,
        #[arg(long)]
        token: String,
        #[arg(long)]
        amount: i128,
        #[arg(long)]
        milestone: String,
        /// Deadline as ISO 8601 (e.g. "2026-12-31T23:59:59Z") or Unix timestamp (seconds).
        #[arg(long)]
        deadline: Option<String>,
    },
    /// Freelancer submits work
    SubmitWork {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "FREELANCER_SECRET")]
        freelancer_secret: String,
    },
    /// Transfer freelancer role to a new address
    TransferFreelancer {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "FREELANCER_SECRET")]
        freelancer_secret: String,
        #[arg(long)]
        new_freelancer: String,
    },
    /// Payer approves milestone and releases payment
    Approve {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "PAYER_SECRET")]
        payer_secret: String,
    },
    /// Payer cancels escrow and gets refund (only before work submitted)
    Cancel {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "PAYER_SECRET")]
        payer_secret: String,
    },
    /// Payer reclaims funds after the deadline has passed
    Expire {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long, env = "PAYER_SECRET")]
        payer_secret: String,
    },
    /// Read current escrow status and full data
    Status {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        /// Token address to include balance in output
        #[arg(long)]
        token: Option<String>,
    },
    /// List all escrows created by a payer address
    List {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,
        #[arg(long)]
        payer: String,
    },

    /// Build (optional) and deploy the escrow contract WASM to the
    /// network
    Deploy {
        /// Path to pre-built WASM file. If omitted, runs `stellar
        /// contract build` first.
        #[arg(long)]
        wasm: Option<std::path::PathBuf>,

        /// Deployer secret key (pays the deployment fee)
        #[arg(long, env = "DEPLOYER_SECRET")]
        deployer_secret: String,

        /// Write the resulting contract ID to a local .env file
        #[arg(long, default_value = ".env")]
        env_file: std::path::PathBuf,
    },

    /// Verify a local WASM file's SHA-256 hash against the on-chain deployed hash
    Verify {
        #[arg(long, env = "ESCROW_CONTRACT_ID")]
        contract_id: String,

        /// Path to the local WASM file to verify
        #[arg(long)]
        wasm: std::path::PathBuf,

        /// Only print the local hash without fetching on-chain (offline mode)
        #[arg(long)]
        local_only: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let as_json = cli.json;
    let dry_run = cli.dry_run;

    let cfg = ConfigFile::load_default_or_explicit(cli.config.as_deref())?;

    let (rpc_url, network_passphrase) = match &cli.network {
        Some(net) => {
            (net.rpc_url().to_string(), net.passphrase().to_string())
        },
        None => (
            cli.rpc_url
                .clone()
                .or(cfg.rpc_url.clone())
                .unwrap_or_else(|| "https://soroban-testnet.stellar.org".to_string()),
            cli.network_passphrase
                .clone()
                .or(cfg.network_passphrase.clone())
                .unwrap_or_else(|| "Test SDF Network ; September 2015".to_string()),
        ),
    };

    match cli.command {
        Commands::Init {
            contract_id,
            admin_secret,
            fee_bps,
            fee_collector,
        } => {
            let admin_addr = stellar_address_from_secret(&admin_secret)?;
            invoke_stellar_cli(
                &rpc_url,
                &network_passphrase,
                &contract_id,
                &admin_secret,
                "init",
                &[
                    "--admin",
                    &admin_addr,
                    "--fee-bps",
                    &fee_bps.to_string(),
                    "--fee-collector",
                    &fee_collector,
                ],
                dry_run,
                as_json,
            )?;
            if !dry_run {
                output(
                    as_json,
                    json!({"status": "ok", "action": "init"}),
                    "Protocol initialised.",
                );
            }
        },
        Commands::Pause {
            contract_id,
            admin_secret,
        } => {
            invoke_stellar_cli(
                &rpc_url,
                &network_passphrase,
                &contract_id,
                &admin_secret,
                "pause",
                &[],
                dry_run,
                as_json,
            )?;
            if !dry_run {
                output(
                    as_json,
                    json!({"status": "ok", "action": "pause"}),
                    "Contract paused.",
                );
            }
        },
        Commands::Unpause {
            contract_id,
            admin_secret,
        } => {
            invoke_stellar_cli(
                &rpc_url,
                &network_passphrase,
                &contract_id,
                &admin_secret,
                "unpause",
                &[],
                dry_run,
                as_json,
            )?;
            if !dry_run {
                output(
                    as_json,
                    json!({"status": "ok", "action": "unpause"}),
                    "Contract unpaused.",
                );
            }
        },
        Commands::Create {
            contract_id,
            payer_secret,
            freelancer,
            token,
            amount,
            milestone,
            deadline,
        } => {
            let payer_addr = stellar_address_from_secret(&payer_secret)?;
            // Accept ISO 8601 string or raw Unix timestamp integer
            let deadline_ts: Option<u64> = match &deadline {
                None => None,
                Some(s) => {
                    let ts = s.parse::<u64>()
                        .ok()
                        .map(Ok)
                        .unwrap_or_else(|| deadline::parse_iso8601_to_timestamp(s))?;
                    Some(ts)
                }
            };
            let deadline_str = deadline_ts.map(|d| d.to_string()).unwrap_or_else(|| "null".into());
            let deadline_human = deadline_ts.map(|d| deadline::format_timestamp(d));
            invoke_stellar_cli(
                &rpc_url,
                &network_passphrase,
                &contract_id,
                &payer_secret,
                "create",
                &[
                    "--payer",
                    &payer_addr,
                    "--freelancer",
                    &freelancer,
                    "--token",
                    &token,
                    "--amount",
                    &amount.to_string(),
                    "--milestone",
                    &milestone,
                    "--deadline",
                    &deadline_str,
                ],
                dry_run,
                as_json,
            )?;
            if !dry_run {
                output(
                    as_json,
                    json!({"status":"ok","action":"create","contract_id":contract_id,"payer":payer_addr,
                           "freelancer":freelancer,"amount":amount,"milestone":milestone,
                           "deadline_ts":deadline_ts,"deadline":deadline_human}),
                    &format!("Escrow created. Funds locked.{}",
                        deadline_human.as_deref().map(|d| format!(" Deadline: {d}")).unwrap_or_default()));
            }
        }
        Commands::SubmitWork { contract_id, freelancer_secret } => {
            invoke_stellar_cli(&rpc_url, &network_passphrase, &contract_id, &freelancer_secret, "submit_work", &[], dry_run, as_json)?;
            if !dry_run {
                output(as_json, json!({"status":"ok","action":"submit_work"}), "Work submitted. Waiting for payer approval.");
            }
        }
        Commands::TransferFreelancer { contract_id, freelancer_secret, new_freelancer } => {
            invoke_stellar_cli(
                &rpc_url,
                &network_passphrase,
                &contract_id,
                &freelancer_secret,
                "transfer_freelancer",
                &["--new-freelancer", &new_freelancer],
                dry_run,
                as_json,
            )?;
            if !dry_run {
                output(
                    as_json,
                    json!({"status":"ok","action":"transfer_freelancer","new_freelancer":new_freelancer}),
                    &format!("Freelancer role transferred to {new_freelancer}."),
                );
            }
        },
        Commands::Approve {
            contract_id,
            payer_secret,
        } => {
            invoke_stellar_cli(
                &rpc_url,
                &network_passphrase,
                &contract_id,
                &payer_secret,
                "approve",
                &[],
                dry_run,
                as_json,
            )?;
            if !dry_run {
                output(
                    as_json,
                    json!({"status":"ok","action":"approve"}),
                    "Payment released to freelancer.",
                );
            }
        },
        Commands::Cancel {
            contract_id,
            payer_secret,
        } => {
            invoke_stellar_cli(
                &rpc_url,
                &network_passphrase,
                &contract_id,
                &payer_secret,
                "cancel",
                &[],
                dry_run,
                as_json,
            )?;
            if !dry_run {
                output(
                    as_json,
                    json!({"status":"ok","action":"cancel"}),
                    "Escrow cancelled. Funds refunded to payer.",
                );
            }
        },
        Commands::Expire {
            contract_id,
            payer_secret,
        } => {
            invoke_stellar_cli(
                &rpc_url,
                &network_passphrase,
                &contract_id,
                &payer_secret,
                "expire",
                &[],
                dry_run,
                as_json,
            )?;
            if !dry_run {
                output(
                    as_json,
                    json!({"status":"ok","action":"expire"}),
                    "Escrow expired. Funds returned to payer.",
                );
            }
        },
        Commands::Status { contract_id, token } => {
            let raw = query_contract(&rpc_url, &network_passphrase, &contract_id, "get_escrow")?;
            let balance: Option<String> = if let Some(ref tok) = token {
                let bal_raw = query_contract_with_args(
                    &rpc_url, &network_passphrase, &contract_id,
                    "get_balance", &["--token", tok],
                )?;
                Some(bal_raw.trim().to_string())
            } else {
                None
            };
            if as_json {
                let parsed: Value = serde_json::from_str(raw.trim())
                    .unwrap_or(Value::String(raw.trim().to_string()));
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({"status":"ok","escrow":parsed}))?
                );
            } else {
                println!("{}", raw.trim());
                if let Some(bal) = balance {
                    println!("balance: {bal}");
                }
            }
        },
        Commands::List { contract_id, payer } => {
            list_escrows(&rpc_url, &network_passphrase, &contract_id, &payer, as_json)?;
        },

        Commands::Deploy {
            wasm,
            deployer_secret,
            env_file,
        } => {
            deploy_contract(
                &rpc_url,
                &network_passphrase,
                wasm.as_deref(),
                &deployer_secret,
                &env_file,
                as_json,
                dry_run,
            )?;
        },

        Commands::Verify { contract_id, wasm, local_only } => {
            run_verify(&rpc_url, &network_passphrase, &contract_id, &wasm, local_only)?;
        }
    }

    Ok(())
}

fn run_verify(
    rpc_url: &str,
    network_passphrase: &str,
    contract_id: &str,
    wasm_path: &std::path::Path,
    local_only: bool,
) -> Result<()> {
    let mut hasher = Sha256::new();
    let wasm_bytes = std::fs::read(wasm_path)
        .with_context(|| format!("Failed to read WASM at {}", wasm_path.display()))?;
    hasher.update(&wasm_bytes);
    let local_hash = hex::encode(hasher.finalize());

    println!("Local WASM hash: {local_hash}");

    if !local_only {
        print!("Fetching on-chain hash for {contract_id}... ");
        let remote_hash = fetch_remote_wasm_hash(rpc_url, network_passphrase, contract_id)?;
        println!("DONE");
        println!("On-chain hash:   {remote_hash}");

        if local_hash == remote_hash {
            println!("✅ VERIFICATION SUCCESS: Local WASM matches on-chain code.");
        } else {
            println!("❌ VERIFICATION FAILED: Hashes do not match!");
            anyhow::bail!("Hash mismatch");
        }
    }
    Ok(())
}

fn fetch_remote_wasm_hash(
    rpc_url: &str,
    network_passphrase: &str,
    contract_id: &str,
) -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args([
            "contract",
            "fetch",
            "--id",
            contract_id,
            "--rpc-url",
            rpc_url,
            "--network-passphrase",
            network_passphrase,
            "--output",
            "wasm",
        ])
        .output()
        .context("Failed to fetch contract WASM from network")?;

    if !out.status.success() {
        anyhow::bail!(
            "stellar contract fetch failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let mut hasher = Sha256::new();
    hasher.update(&out.stdout);
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

fn deploy_contract(
    rpc_url: &str,
    network_passphrase: &str,
    wasm: Option<&std::path::Path>,
    deployer_secret: &str,
    env_file: &std::path::Path,
    as_json: bool,
    dry_run: bool,
) -> Result<()> {
    let wasm_path = match wasm {
        Some(p) => p.to_path_buf(),
        None => {
            eprintln!("No --wasm provided; running `stellar contract build`…");
            let status = std::process::Command::new("stellar")
                .args(["contract", "build"])
                .status()
                .context("stellar CLI not found")?;
            if !status.success() {
                anyhow::bail!("`stellar contract build` failed");
            }
            std::path::PathBuf::from("target/wasm32-unknown-unknown/release/escrow.wasm")
        },
    };

    if !wasm_path.exists() {
        anyhow::bail!("WASM file not found: {}", wasm_path.display());
    }

    let mut args = vec![
        "contract",
        "deploy",
        "--wasm",
        wasm_path.to_str().context("invalid wasm path")?,
        "--source",
        deployer_secret,
        "--rpc-url",
        rpc_url,
        "--network-passphrase",
        network_passphrase,
    ];

    if dry_run {
        args.push("--sim-only");
    }

    let out = std::process::Command::new("stellar")
        .args(&args)
        .output()
        .context("stellar CLI not found")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("Deployment failed: {stderr}");
    }

    let contract_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if contract_id.is_empty() {
        anyhow::bail!("Deployment succeeded but no contract ID was returned");
    }

    if dry_run {
        output_dry_run(as_json, "deploy", &contract_id, &String::from_utf8_lossy(&out.stderr));
        return Ok(());
    }

    upsert_env_var(env_file, "ESCROW_CONTRACT_ID", &contract_id)?;

    output(
        as_json,
        serde_json::json!({"status": "ok", "contract_id": contract_id, "env_file": env_file.display().to_string()}),
        &format!("Deployed! Contract ID: {contract_id}\\nWritten to {}", env_file.display()),
    );
    Ok(())
}

fn upsert_env_var(path: &std::path::Path, key: &str, value: &str) -> Result<()> {
    use std::io::Write as _;
    let existing = if path.exists() {
        std::fs::read_to_string(path).context("reading .env file")?
    } else {
        String::new()
    };
    let prefix = format!("{key}=");
    let new_line = format!("{key}={value}");
    let mut found = false;
    let updated: String = existing.lines().map(|line| {
        if line.starts_with(&prefix) { found = true; new_line.clone() } else { line.to_string() }
    }).collect::<Vec<_>>().join("\n");
    let mut content = if found { updated } else { format!("{existing}\n{new_line}") };
    if !content.ends_with('\n') { content.push('\n'); }
    let mut file = std::fs::File::create(path).context("writing .env file")?;
    file.write_all(content.as_bytes()).context("writing .env file")?;
    Ok(())
}

fn stellar_address_from_secret(secret: &str) -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args(["keys", "address", secret])
        .output()
        .context("stellar CLI not found")?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn invoke_stellar_cli(
    rpc_url: &str,
    network_passphrase: &str,
    contract_id: &str,
    secret: &str,
    function: &str,
    extra_args: &[&str],
    dry_run: bool,
    as_json: bool,
) -> Result<()> {
    let mut args = vec![
        "contract",
        "invoke",
        "--id",
        contract_id,
        "--rpc-url",
        rpc_url,
        "--network-passphrase",
        network_passphrase,
        "--source",
        secret,
    ];
    if dry_run { args.push("--sim-only"); }
    args.push("--");
    args.push(function);
    args.extend_from_slice(extra_args);

    let out = if dry_run {
        std::process::Command::new("stellar").args(&args).output()?
    } else {
        let status = std::process::Command::new("stellar").args(&args).status()?;
        if !status.success() { anyhow::bail!("stellar CLI failed"); }
        return Ok(());
    };

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    output_dry_run(as_json, function, &stdout, &stderr);
    if !out.status.success() { anyhow::bail!("Simulation failed"); }
    Ok(())
}

fn query_contract(rpc_url: &str, network_passphrase: &str, contract_id: &str, function: &str) -> Result<String> {
    let out = std::process::Command::new("stellar").args(["contract", "invoke", "--id", contract_id, "--rpc-url", rpc_url, "--network-passphrase", network_passphrase, "--", function]).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn query_contract_with_args(rpc_url: &str, network_passphrase: &str, contract_id: &str, function: &str, args: &[&str]) -> Result<String> {
    let mut cmd_args = vec!["contract", "invoke", "--id", contract_id, "--rpc-url", rpc_url, "--network-passphrase", network_passphrase, "--", function];
    cmd_args.extend_from_slice(args);
    let out = std::process::Command::new("stellar").args(&cmd_args).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn list_escrows(rpc_url: &str, network_passphrase: &str, contract_id: &str, payer: &str, as_json: bool) -> Result<()> {
    let events = fetch_events(rpc_url, network_passphrase, contract_id)?;
    let escrows: Vec<Value> = events.into_iter()
        .filter(|e| e["topic"][0].as_str().unwrap_or("") == "escrow_created" && e["value"][0].as_str().unwrap_or("") == payer)
        .map(|e| json!({"contract_id": contract_id,"payer": e["value"][0],"freelancer": e["value"][1],"amount": e["value"][2],"milestone": e["value"][3]}))
        .collect();
    if as_json {
        println!("{}", serde_json::to_string_pretty(&json!({"escrows": escrows}))?);
    } else if escrows.is_empty() {
        println!("No escrows found for payer {payer}");
    } else {
        println!("Escrows for payer {payer}:");
        for (i, e) in escrows.iter().enumerate() {
            println!("  [{}] contract={} milestone={} amount={} freelancer={}", i + 1, e["contract_id"].as_str().unwrap_or("-"), e["milestone"].as_str().unwrap_or("-"), e["amount"], e["freelancer"].as_str().unwrap_or("-"));
        }
    }
    Ok(())
}

fn fetch_events(rpc_url: &str, network_passphrase: &str, contract_id: &str) -> Result<Vec<Value>> {
    let out = std::process::Command::new("stellar").args(["contract", "events", "--id", contract_id, "--rpc-url", rpc_url, "--network-passphrase", network_passphrase, "--output", "json"]).output()?;
    Ok(serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap_or_default())
}

fn output_dry_run(as_json: bool, function: &str, sim_stdout: &str, sim_stderr: &str) {
    let res: Value = serde_json::from_str(sim_stdout.trim()).unwrap_or(Value::String(sim_stdout.trim().to_string()));
    if as_json {
        println!("{}", serde_json::to_string_pretty(&json!({"dry_run":true,"simulation":true,"action":function,"result":res})).unwrap());
    } else {
        println!("═══ DRY-RUN SIMULATION ═══\\nAction: {function}\\nStatus: simulated\\n");
        if let Value::Object(m) = &res { for (k, v) in m { println!("  {k}: {v}"); } } else { println!("  Result: {res}"); }
        if !sim_stderr.trim().is_empty() { println!("\\n  Diagnostics:\\n    {}", sim_stderr.trim()); }
        println!("\\n⚠ This was a dry-run simulation.");
    }
}

fn output(as_json: bool, data: Value, human: &str) {
    if as_json { println!("{}", serde_json::to_string_pretty(&data).unwrap()); } else { println!("{human}"); }
}

mod deadline;
mod keypair;
mod wasm_hash;
mod xdr;


#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test]
    fn test_dry_run_flag_parsing() {
        let cli = Cli::try_parse_from(["star-escrow", "--dry-run", "status", "--contract-id", "C1"]).unwrap();
        assert!(cli.dry_run);
    }
    #[test]
    fn test_dry_run_flag_absent() {
        let cli = Cli::try_parse_from(["star-escrow", "status", "--contract-id", "C1"]).unwrap();
        assert!(!cli.dry_run);
    }
    #[test]
    fn test_dry_run_with_json_flag() {
        let cli = Cli::try_parse_from(["star-escrow", "--dry-run", "--json", "status", "--contract-id", "C1"]).unwrap();
        assert!(cli.dry_run);
        assert!(cli.json);
    }
    #[test]
    fn test_dry_run_with_create_command() {
        let cli = Cli::try_parse_from(["star-escrow", "--dry-run", "create", "--contract-id", "C1", "--payer-secret", "S1", "--freelancer", "G1", "--token", "T1", "--amount", "100", "--milestone", "M1"]).unwrap();
        assert!(cli.dry_run);
        if let Commands::Create { amount, .. } = cli.command { assert_eq!(amount, 100); } else { panic!("Expected Create"); }
    }
    #[test]
    fn test_dry_run_with_approve_command() {
        let cli = Cli::try_parse_from(["star-escrow", "--dry-run", "approve", "--contract-id", "C1", "--payer-secret", "S1"]).unwrap();
        assert!(cli.dry_run);
        assert!(matches!(cli.command, Commands::Approve { .. }));
    }
    #[test]
    fn test_dry_run_with_cancel_command() {
        let cli = Cli::try_parse_from(["star-escrow", "--dry-run", "cancel", "--contract-id", "C1", "--payer-secret", "S1"]).unwrap();
        assert!(cli.dry_run);
        assert!(matches!(cli.command, Commands::Cancel { .. }));
    }
    #[test]
    fn test_dry_run_with_submit_work_command() {
        let cli = Cli::try_parse_from(["star-escrow", "--dry-run", "submit-work", "--contract-id", "C1", "--freelancer-secret", "S1"]).unwrap();
        assert!(cli.dry_run);
        assert!(matches!(cli.command, Commands::SubmitWork { .. }));
    }
    #[test]
    fn test_dry_run_flag_after_subcommand() {
        let cli = Cli::try_parse_from(["star-escrow", "approve", "--dry-run", "--contract-id", "C1", "--payer-secret", "S1"]).unwrap();
        assert!(cli.dry_run);
    }
    #[test]
    fn test_network_rpc_urls() {
        assert_eq!(Network::Testnet.rpc_url(), "https://soroban-testnet.stellar.org");
        assert_eq!(Network::Mainnet.rpc_url(), "https://soroban-mainnet.stellar.org");
    }
}

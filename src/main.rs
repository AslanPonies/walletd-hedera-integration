// src/main.rs

mod mainnet;
use anyhow::Result;
use env_logger::Env;
use hedera::AccountId;
use log::{error, info, LevelFilter};
use std::io::{self, Write};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the logger with default level 'info'
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .filter_level(LevelFilter::Info)
        .init();

    if let Err(e) = run_application().await {
        error!("Application error: {}", e);
    }

    Ok(())
}

async fn run_application() -> Result<()> {
    println!("Enter your choice:");
    println!("1: Fetch Account Info");
    println!("2: Create New Account");
    println!("3: Send hBars");
    println!("4: Transfer Tokens");
    println!("5: Deploy Smart Contract");
    print!("Choice: ");
    io::stdout().flush()?;

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let choice = choice.trim();

    match choice {
        "1" => {
            info!("Option 1 selected.");
            // Prompt user for account ID
            print!("Enter Account ID (e.g., 0.0.1234): ");
            io::stdout().flush()?;
            let mut account_id_str = String::new();
            io::stdin().read_line(&mut account_id_str)?;
            let account_id_str = account_id_str.trim();
            let account_id = AccountId::from_str(account_id_str)?;
            mainnet::fetch_account_info(account_id).await?
        }
        "2" => {
            info!("Option 2 selected.");
            mainnet::create_new_account().await?
        }
        "3" => {
            info!("Option 3 selected.");
            mainnet::send_hbars().await?
        }
        "4" => {
            info!("Option 4 selected.");
            mainnet::transfer_tokens().await?
        }
        "5" => {
            info!("Option 5 selected.");
            mainnet::deploy_smart_contract().await?
        }
        _ => println!("Invalid choice"),
    }

    Ok(())
}

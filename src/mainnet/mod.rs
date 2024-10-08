// src/mainnet/mod.rs

use anyhow::{Context, Result};
use dotenv::dotenv;
use hedera::{
    AccountBalanceQuery, AccountCreateTransaction, AccountId, AccountInfoQuery, Client,
    ContractCreateTransaction, ContractFunctionParameters, Error as HederaError,
    FileAppendTransaction, FileCreateTransaction, Hbar, PrivateKey, Status,
    TokenAssociateTransaction, TokenId, TransactionReceipt, TransactionRecord,
    TransactionResponse, TransferTransaction,
};
use log::{error, info, warn};
use std::collections::HashMap;
use std::convert::TryInto;
use std::env;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use tokio::time::{sleep, Duration};

// Function to initialize the Hedera client with custom settings
fn init_client() -> Result<Client> {
    info!("Initializing client...");
    dotenv().ok();

    let operator_id = AccountId::from_str(&env::var("OPERATOR_ID")?)?;
    info!("Operator ID loaded: {}", operator_id);

    let operator_key = PrivateKey::from_str(&env::var("OPERATOR_PRIVATE_KEY")?)?;
    info!("Operator Key loaded.");

    // Initialize the client for mainnet
    let client = Client::for_mainnet();

    // Set the operator
    client.set_operator(operator_id, operator_key.clone());

    // Manually specify healthy nodes with correct ports
    let mut network = HashMap::new();

    // Use port 50211 for gRPC communication
    network.insert("35.237.200.180:50211".to_string(), AccountId::from(3));  // Node 0.0.3
    network.insert("35.186.191.247:50211".to_string(), AccountId::from(4));  // Node 0.0.4
    network.insert("35.192.2.25:50211".to_string(), AccountId::from(5));     // Node 0.0.5
    network.insert("35.199.161.108:50211".to_string(), AccountId::from(6));  // Node 0.0.6
    network.insert("35.236.5.219:50211".to_string(), AccountId::from(8));    // Node 0.0.8
    network.insert("35.242.233.154:50211".to_string(), AccountId::from(10)); // Node 0.0.10
    network.insert("35.240.118.96:50211".to_string(), AccountId::from(11));  // Node 0.0.11
    network.insert("35.204.86.32:50211".to_string(), AccountId::from(12));   // Node 0.0.12

    client.set_network(network)?;
    info!("Manually configured network with updated nodes and port 50211.");

    // Increase timeouts and attempts
    client.set_request_timeout(Some(Duration::from_secs(300)));
    client.set_max_attempts(20);

    info!("Client initialized.");

    Ok(client)
}

// Helper function to get transaction receipt with retry logic
async fn get_receipt_with_retry(
    client: &Client,
    transaction: &TransactionResponse,
) -> Result<TransactionReceipt> {
    let mut attempts = 0;
    loop {
        match transaction.get_receipt(client).await {
            Ok(receipt) => {
                // Return the receipt even if the status is not Success
                return Ok(receipt);
            }
            Err(HederaError::ReceiptStatus { status, transaction_id }) => {
                // Handle specific statuses that are acceptable
                if status == Status::TokenAlreadyAssociatedToAccount {
                    let txn_id_str = transaction_id
                        .as_ref()
                        .map(|tid| tid.to_string())
                        .unwrap_or_else(|| "Unknown".to_string());

                    warn!(
                        "Received TokenAlreadyAssociatedToAccount for transaction `{}`",
                        txn_id_str
                    );

                    // Create a receipt with the status and return it
                    let receipt = TransactionReceipt {
                        status,
                        transaction_id: transaction_id.map(|tid| *tid),
                        account_id: None,
                        file_id: None,
                        contract_id: None,
                        topic_id: None,
                        token_id: None,
                        schedule_id: None,
                        serials: vec![],
                        topic_sequence_number: 0,
                        topic_running_hash: None,
                        topic_running_hash_version: 0,
                        total_supply: 0,
                        scheduled_transaction_id: None,
                        children: vec![],
                        duplicates: vec![],
                        node_id: 0,
                    };
                    return Ok(receipt);
                } else {
                    let txn_id_str = transaction_id
                        .as_ref()
                        .map(|tid| tid.to_string())
                        .unwrap_or_else(|| "Unknown".to_string());

                    error!(
                        "Transaction `{}` failed with status `{:?}`",
                        txn_id_str, status
                    );
                    return Err(anyhow::anyhow!(
                        "Transaction `{}` failed with status `{:?}`",
                        txn_id_str,
                        status
                    ));
                }
            }
            Err(e) if e.to_string().contains("Transient") && attempts < 5 => {
                attempts += 1;
                warn!(
                    "Transient error fetching receipt: {}. Retrying (attempt {})...",
                    e, attempts
                );
                sleep(Duration::from_secs(2_u64.pow(attempts))).await;
            }
            Err(e) => {
                error!("Failed to get receipt after {} attempts: {}", attempts, e);
                return Err(e.into());
            }
        }
    }
}

// Helper function to get transaction record with retry logic
async fn get_record_with_retry(
    client: &Client,
    transaction: &TransactionResponse,
) -> Result<TransactionRecord> {
    let mut attempts = 0;
    loop {
        match transaction.get_record(client).await {
            Ok(record) => return Ok(record),
            Err(e) if e.to_string().contains("Transient") && attempts < 5 => {
                attempts += 1;
                warn!(
                    "Transient error fetching record: {}. Retrying (attempt {})...",
                    e, attempts
                );
                sleep(Duration::from_secs(2_u64.pow(attempts))).await;
            }
            Err(e) => {
                error!("Failed to get record after {} attempts: {}", attempts, e);
                return Err(e.into());
            }
        }
    }
}

// Function to fetch account information on Hedera
pub async fn fetch_account_info(account_id: AccountId) -> Result<()> {
    info!("Fetching account info for account ID: {}", account_id);

    let client = init_client()?;
    info!("Client initialized.");

    let account_info = AccountInfoQuery::new()
        .account_id(account_id)
        .execute(&client)
        .await;

    // Handle possible errors
    match account_info {
        Ok(info) => {
            info!("Account ID: {}", info.account_id);
            info!("Public Key: {:?}", info.key);
            info!("Balance: {}", info.balance.to_string());
            info!("Expiration Time: {:?}", info.expiration_time);
            // info!("Staking Info: {:?}", info.staking_info); // Commented out as staking_info may not exist
        }
        Err(e) => {
            error!("Error occurred: {}", e);
            if e.to_string().contains("INSUFFICIENT_PAYER_BALANCE") {
                error!("Error: Insufficient funds to perform this transaction.");
            } else if e.to_string().contains("Transient") {
                warn!("Warning: There was a transient issue fetching account information. Please retry or check the wallet for up-to-date information.");
            } else {
                error!("Error: Failed to fetch account information due to: {}", e);
            }
        }
    }

    Ok(())
}

// Function to create a new account on Hedera
pub async fn create_new_account() -> Result<()> {
    info!("create_new_account function has been called.");

    let client = init_client()?;
    info!("Client initialized.");

    let new_key = PrivateKey::generate_ed25519();
    info!("New key generated.");

    // Fetch the operator account balance
    let operator_account_id = client.get_operator_account_id().unwrap();
    let balance = AccountBalanceQuery::new()
        .account_id(operator_account_id)
        .execute(&client)
        .await?;
    info!("Operator account balance: {}", balance.hbars);

    // Set the initial balance for the new account
    let initial_balance = Hbar::new(10); // Adjust as needed

    // Estimate the transaction fee (set to a conservative value)
    let estimated_fee = Hbar::new(1); // Adjust as needed

    // Check if the balance is sufficient
    if balance.hbars < initial_balance + estimated_fee {
        error!(
            "Insufficient operator account balance. Required: {}, Available: {}",
            initial_balance + estimated_fee,
            balance.hbars
        );
        return Err(anyhow::anyhow!("Insufficient operator account balance"));
    }

    // Create the new account transaction
    let transaction = AccountCreateTransaction::new()
        .key(new_key.public_key())
        .initial_balance(initial_balance)
        .max_transaction_fee(estimated_fee)
        .execute(&client)
        .await?;

    // Get the receipt with retry logic
    let receipt = get_receipt_with_retry(&client, &transaction).await?;

    let new_account_id = receipt.account_id.unwrap();
    info!("New Account ID: {}", new_account_id);
    info!("New Account Private Key: {}", new_key);
    info!("New Account Public Key: {}", new_key.public_key());

    Ok(())
}

// Function to send hbars between accounts on Hedera
pub async fn send_hbars() -> Result<()> {
    info!("send_hbars function has been called.");

    let client = init_client()?;
    info!("Client initialized.");

    // Retrieve the recipient account ID from the environment variables
    let recipient_account = AccountId::from_str(&env::var("RECIPIENT_ID")?)?;
    info!("Recipient account ID parsed: {}", recipient_account);

    // Fetch the operator account balance
    let operator_account_id = client.get_operator_account_id().unwrap();
    let balance = AccountBalanceQuery::new()
        .account_id(operator_account_id)
        .execute(&client)
        .await?;
    info!("Operator account balance: {}", balance.hbars);

    // Set the amount to send
    let amount_to_send = Hbar::new(10);

    // Estimate the transaction fee (set to a conservative value)
    let estimated_fee = Hbar::new(1);

    // Check if the balance is sufficient
    if balance.hbars < amount_to_send + estimated_fee {
        error!(
            "Insufficient operator account balance. Required: {}, Available: {}",
            amount_to_send + estimated_fee,
            balance.hbars
        );
        return Err(anyhow::anyhow!("Insufficient operator account balance"));
    }

    // Send hbars
    let transaction = TransferTransaction::new()
        .hbar_transfer(operator_account_id, -amount_to_send)
        .hbar_transfer(recipient_account, amount_to_send)
        .max_transaction_fee(estimated_fee)
        .execute(&client)
        .await?;

    // Get the receipt with retry logic
    let receipt = get_receipt_with_retry(&client, &transaction).await?;

    info!("Transfer Status: {:?}", receipt.status);

    Ok(())
}

// Function to check token balance of an account
pub async fn check_token_balance(account_id: AccountId, token_id: TokenId) -> Result<u64> {
    let client = init_client()?;

    let balance = AccountBalanceQuery::new()
        .account_id(account_id)
        .execute(&client)
        .await?;

    let token_balance = balance.tokens.get(&token_id).cloned().unwrap_or(0);
    info!(
        "Account {} has {} units of token {}",
        account_id, token_balance, token_id
    );

    Ok(token_balance)
}

// Function to associate the operator account with a token
pub async fn associate_operator_with_token() -> Result<()> {
    info!("Associating operator account with token.");

    let client = init_client()?;
    let operator_account_id = client.get_operator_account_id().unwrap();

    // Retrieve the token ID from the environment variables
    let token_id = TokenId::from_str(&env::var("TOKEN_ID")?)?;

    // Fetch the operator account balance
    let balance = AccountBalanceQuery::new()
        .account_id(operator_account_id)
        .execute(&client)
        .await?;
    info!("Operator account balance: {}", balance.hbars);

    // Estimate the transaction fee (set to a conservative value)
    let estimated_fee = Hbar::new(1); // Adjust as needed

    // Check if the balance is sufficient
    if balance.hbars < estimated_fee {
        error!(
            "Insufficient operator account balance. Required: {}, Available: {}",
            estimated_fee,
            balance.hbars
        );
        return Err(anyhow::anyhow!("Insufficient operator account balance"));
    }

    // Associate the operator account with the token
    let transaction = TokenAssociateTransaction::new()
        .account_id(operator_account_id)
        .token_ids([token_id])
        .max_transaction_fee(estimated_fee)
        .execute(&client)
        .await?;

    // Get the receipt with retry logic
    let receipt = get_receipt_with_retry(&client, &transaction).await?;

    match receipt.status {
        Status::TokenAlreadyAssociatedToAccount => {
            warn!("Operator account is already associated with the token.");
        }
        Status::Success => {
            info!("Operator Token Association Status: Success");
        }
        _ => {
            error!(
                "Operator Token Association failed with status: {:?}",
                receipt.status
            );
            return Err(anyhow::anyhow!(
                "Operator Token Association failed with status: {:?}",
                receipt.status
            ));
        }
    }

    Ok(())
}

// Function to associate the recipient account with a token
pub async fn associate_recipient_with_token() -> Result<()> {
    info!("Associating recipient account with token.");

    // Retrieve the token ID and recipient account ID from the environment variables
    let token_id = TokenId::from_str(&env::var("TOKEN_ID")?)?;
    let recipient_account_id = AccountId::from_str(&env::var("RECIPIENT_ID")?)?;

    // Retrieve the recipient's private key
    let recipient_private_key_str = env::var("RECIPIENT_PRIVATE_KEY")
        .expect("RECIPIENT_PRIVATE_KEY must be set in the .env file");
    let recipient_private_key = PrivateKey::from_str(&recipient_private_key_str)
        .expect("Invalid recipient private key format");

    // Initialize the operator client
    let client = init_client()?;
    let operator_account_id = client.get_operator_account_id().unwrap();

    // Fetch the operator account balance
    let balance = AccountBalanceQuery::new()
        .account_id(operator_account_id)
        .execute(&client)
        .await?;
    info!("Operator account balance: {}", balance.hbars);

    // Estimate the transaction fee (set to a conservative value)
    let estimated_fee = Hbar::new(1); // Adjust as needed

    // Check if the balance is sufficient
    if balance.hbars < estimated_fee {
        error!(
            "Insufficient operator account balance. Required: {}, Available: {}",
            estimated_fee,
            balance.hbars
        );
        return Err(anyhow::anyhow!("Insufficient operator account balance"));
    }

    // Build the transaction step by step
    let mut transaction1 = TokenAssociateTransaction::new();
    transaction1.account_id(recipient_account_id);
    transaction1.token_ids([token_id]);
    let transaction2 = transaction1.freeze_with(&client)?;
    let transaction3 = transaction2.sign(recipient_private_key.clone());

    // Execute the transaction (operator pays the fee)
    let transaction_response = transaction3.execute(&client).await?;

    // Get the receipt with retry logic
    let receipt = get_receipt_with_retry(&client, &transaction_response).await?;

    match receipt.status {
        Status::TokenAlreadyAssociatedToAccount => {
            warn!("Recipient account is already associated with the token.");
        }
        Status::Success => {
            info!("Recipient Token Association Status: Success");
        }
        _ => {
            error!(
                "Recipient Token Association failed with status: {:?}",
                receipt.status
            );
            return Err(anyhow::anyhow!(
                "Recipient Token Association failed with status: {:?}",
                receipt.status
            ));
        }
    }

    Ok(())
}

// Function to transfer tokens between accounts on Hedera
pub async fn transfer_tokens() -> Result<()> {
    info!("transfer_tokens function has been called.");

    // Associate the operator account with the token
    associate_operator_with_token().await?;

    // Associate the recipient account with the token
    associate_recipient_with_token().await?;

    let client = init_client()?;
    info!("Client initialized.");

    // Retrieve the token ID and recipient account ID from the environment variables
    let token_id = TokenId::from_str(&env::var("TOKEN_ID")?)?;
    let recipient_account = AccountId::from_str(&env::var("RECIPIENT_ID")?)?;

    info!(
        "Token ID: {}, Recipient Account ID: {}",
        token_id, recipient_account
    );

    // Check operator's token balance
    let operator_account_id = client.get_operator_account_id().unwrap();
    let operator_token_balance = check_token_balance(operator_account_id, token_id).await?;
    if operator_token_balance == 0 {
        error!(
            "Insufficient token balance. Operator has {} units of token {}.",
            operator_token_balance, token_id
        );
        return Err(anyhow::anyhow!("Insufficient token balance"));
    }

    // Set the amount to transfer (up to 100 tokens)
    let amount_to_transfer = std::cmp::min(100, operator_token_balance);

    // Fetch the operator account hbar balance
    let balance = AccountBalanceQuery::new()
        .account_id(operator_account_id)
        .execute(&client)
        .await?;
    info!("Operator account hbar balance: {}", balance.hbars);

    // Estimate the transaction fee (set to a conservative value)
    let estimated_fee = Hbar::new(1); // Adjust as needed

    // Check if the balance is sufficient
    if balance.hbars < estimated_fee {
        error!(
            "Insufficient operator account hbar balance. Required: {}, Available: {}",
            estimated_fee,
            balance.hbars
        );
        return Err(anyhow::anyhow!("Insufficient operator account hbar balance"));
    }

    // Transfer tokens
    let transaction = TransferTransaction::new()
        .token_transfer(token_id, operator_account_id, -(amount_to_transfer as i64)) // Operator sends tokens
        .token_transfer(token_id, recipient_account, amount_to_transfer as i64) // Recipient receives tokens
        .max_transaction_fee(estimated_fee)
        .execute(&client)
        .await?;

    // Get the receipt with retry logic
    let receipt = get_receipt_with_retry(&client, &transaction).await?;

    info!("Token Transfer Status: {:?}", receipt.status);

    Ok(())
}

// Function to deploy a smart contract on Hedera
pub async fn deploy_smart_contract() -> Result<()> {
    info!("deploy_smart_contract function has been called.");

    let client = init_client()?;
    info!("Client initialized.");

    // Fetch the operator account balance
    let operator_account_id = client.get_operator_account_id().unwrap();
    let balance = AccountBalanceQuery::new()
        .account_id(operator_account_id)
        .execute(&client)
        .await?;
    info!("Operator account balance: {}", balance.hbars);

    // Set a reasonable gas limit based on your available balance
    let available_hbars = balance.hbars.to_tinybars() as u64;

    // Estimate the transaction fee (e.g., 1 hbar per 100,000 gas)
    let gas_limit_per_hbar = 100_000;
    let max_gas_limit = (available_hbars / 1_000_000) * gas_limit_per_hbar; // Convert tinybars to hbars

    // Ensure gas limit is at least a minimum value
    let gas_limit = std::cmp::max(max_gas_limit, 100_000); // Minimum gas limit
    info!("Setting gas limit to: {}", gas_limit);

    // Calculate an appropriate max transaction fee
    let fee_tinybars_u64 = (gas_limit / gas_limit_per_hbar) * 1_000_000; // 1 hbar per 100,000 gas
    let fee_tinybars_i64 = fee_tinybars_u64.try_into().expect("Fee exceeds i64::MAX");
    let estimated_fee = Hbar::from_tinybars(fee_tinybars_i64);
    info!("Setting max transaction fee to: {}", estimated_fee);

    // Check if the balance is sufficient
    if balance.hbars < estimated_fee {
        error!(
            "Insufficient operator account balance. Required: {}, Available: {}",
            estimated_fee, balance.hbars
        );
        return Err(anyhow::anyhow!("Insufficient operator account balance"));
    }

    // Load the Solidity contract bytecode
    let bytecode_path = Path::new("contracts/MyContract.bin");
    let bytecode = fs::read(&bytecode_path).with_context(|| {
        format!(
            "Failed to read smart contract bytecode file: {:?}",
            bytecode_path
        )
    })?;
    info!(
        "Contract bytecode loaded successfully. Size: {} bytes",
        bytecode.len()
    );

    // Upload the bytecode to Hedera File Service
    info!("Uploading bytecode to Hedera File Service...");

    // Create the file on Hedera File Service
    let initial_bytecode_chunk = &bytecode[..usize::min(4096, bytecode.len())];
    let file_create_tx = FileCreateTransaction::new()
        .keys([client.get_operator_public_key().unwrap()])
        .contents(initial_bytecode_chunk) // Initial contents
        .max_transaction_fee(estimated_fee.clone()) // Set max fee
        .execute(&client)
        .await?;

    let file_create_receipt = get_receipt_with_retry(&client, &file_create_tx).await?;
    let bytecode_file_id = file_create_receipt.file_id.unwrap();
    info!("Bytecode file created with ID: {}", bytecode_file_id);

    // Append remaining bytecode if necessary
    if bytecode.len() > 4096 {
        info!("Bytecode size exceeds 4096 bytes. Appending remaining data...");
        let mut offset = 4096;
        while offset < bytecode.len() {
            let end = usize::min(offset + 4096, bytecode.len());
            let chunk = &bytecode[offset..end];

            let file_append_tx = FileAppendTransaction::new()
                .file_id(bytecode_file_id)
                .contents(chunk)
                .max_transaction_fee(estimated_fee.clone()) // Set max fee
                .execute(&client)
                .await?;

            get_receipt_with_retry(&client, &file_append_tx).await?;
            info!("Appended bytes {} to {}.", offset, end);
            offset = end;
        }
    }

    // Prepare constructor parameters if needed
    let constructor_params = ContractFunctionParameters::new(); // No parameters added

    // Deploy the contract using the bytecode file ID and constructor parameters
    let transaction = ContractCreateTransaction::new()
        .bytecode_file_id(bytecode_file_id)
        .gas(gas_limit)
        .admin_key(client.get_operator_public_key().unwrap())
        .constructor_parameters(constructor_params.to_bytes(None))
        .max_transaction_fee(estimated_fee)
        .execute(&client)
        .await?;

    // Get the transaction receipt with retry logic
    let receipt = get_receipt_with_retry(&client, &transaction).await?;

    info!("Transaction Receipt Status: {:?}", receipt.status);

    // Get the transaction record with retry logic
    let record = get_record_with_retry(&client, &transaction).await?;

    // Handle the contract function result and error messages
    if let Some(contract_id) = receipt.contract_id {
        info!("Smart Contract deployed with ID: {}", contract_id);
        Ok(())
    } else {
        error!("Contract deployment failed.");
        error!("Transaction Receipt Status: {:?}", receipt.status);

        // If available, get the contract function result
        if let Some(contract_function_result) = &record.contract_function_result {
            if let Some(error_message) = &contract_function_result.error_message {
                if !error_message.is_empty() {
                    error!(
                        "Contract Function Result Error Message: {}",
                        error_message
                    );
                } else {
                    error!("Error message is empty.");
                }
            } else {
                error!("No error message in contract function result.");
            }
        } else {
            error!("No contract function result available.");
        }

        Err(anyhow::anyhow!(
            "Failed to deploy smart contract. Transaction status: {:?}",
            receipt.status
        ))
    }
}

use anyhow::{anyhow, Result};
use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
//use std::fs::File;
//use std::io::Write;
use clap::Parser;
use std::net::IpAddr;
use std::time::Instant;
use ureq;

#[derive(Parser, Debug)]
#[clap(version, about = "solana getBlock proxy")]
struct Args {
    #[clap(short, long)]
    address: IpAddr,
    #[clap(short, long)]
    port: u16,
}

fn has_sol_transfer(tx: &ureq::serde_json::Value) -> Result<bool> {
    let pre_balances = tx["meta"]["preBalances"]
        .as_array()
        .ok_or(anyhow!("meta.preBalances is none"))?;
    let post_balances = tx["meta"]["postBalances"]
        .as_array()
        .ok_or(anyhow!("meta.postBalances is none"))?;
    if pre_balances.len() != post_balances.len() {
        return Err(anyhow!("pre,post_balances size is not match"));
    }

    for (pre, post) in pre_balances.iter().zip(post_balances.iter()) {
        let pre = pre.as_u64().ok_or(anyhow!("pre is invalid ?"))?;
        let post = post.as_u64().ok_or(anyhow!("post is invalid ?"))?;
        if pre < post {
            // sol transfer tx
            return Ok(true);
        }
    }

    Ok(false)
}

fn has_token_transfer(tx: &ureq::serde_json::Value) -> Result<bool> {
    let pre_token_balances = tx["meta"]["preTokenBalances"]
        .as_array()
        .ok_or(anyhow!("meta.preTokenBalances is none"))?;
    let post_token_balances = tx["meta"]["postTokenBalances"]
        .as_array()
        .ok_or(anyhow!("meta.postTokenBalances is none"))?;

    if post_token_balances.len() == 0 {
        // no token transfer
        return Ok(false);
    }

    if pre_token_balances.len() < post_token_balances.len() {
        // assuming includes create account instruction
        return Ok(true);
    }

    for (pre, post) in pre_token_balances.iter().zip(post_token_balances.iter()) {
        let pre = pre["uiTokenAmount"]["amount"]
            .as_str()
            .ok_or(anyhow!("pre.uiTokenAmount.amount is invalid ?"))?;
        let post = post["uiTokenAmount"]["amount"]
            .as_str()
            .ok_or(anyhow!("post.uiTokenAmount.amount is invalid ?"))?;

        let pre = pre.parse::<u64>()?;
        let post = post.parse::<u64>()?;

        if pre < post {
            // token transfer tx
            return Ok(true);
        }
    }

    Ok(false)
}

async fn filter_tx(block: &ureq::serde_json::Value) -> Result<ureq::serde_json::Value> {
    if block["result"].is_null() {
        return Err(anyhow!("block.result is null"));
    }

    if block["result"]["transactions"].is_null() {
        return Err(anyhow!("block.result.transactions is null"));
    }

    let transactions = block["result"]["transactions"]
        .as_array()
        .ok_or(anyhow!("transactions is not array ?"))?;

    // filter no balance changed tx
    let mut transfer_transactions: Vec<ureq::serde_json::Value> = Vec::new();
    for tx in transactions {
        if has_sol_transfer(tx)? || has_token_transfer(tx)? {
            transfer_transactions.push(tx.to_owned());
        }
    }

    println!("{}", transactions.len());
    println!("{}", transfer_transactions.len());

    let response: ureq::serde_json::Value = ureq::json!(
        {
            "id": block["id"],
            "jsonrpc": block["jsonrpc"],
            "result":
                {
                "blockHeight": block["result"]["blockHeight"],
                "blockTime": block["result"]["blockTime"],
                "blockhash": block["result"]["blockhash"],
                "parentSlot": block["result"]["parentSlot"],
                "previousBlockhash": block["result"]["previousBlockhash"],
                "transactions": transfer_transactions,
            }
        }
    );

    Ok(response)
}

async fn get_block(
    Json(input): Json<ureq::serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    // getBlock from node
    let start = Instant::now();
    let url = "https://api.mainnet-beta.solana.com";
    let response = ureq::post(&url)
        .send_json(input)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response: ureq::serde_json::Value = response
        .into_json()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let duration = start.elapsed();
    println!("get_block: {:?}", duration);

    /*
    {
        let mut file = File::create(format!("block.json")).unwrap();
        let response_str = response.to_string();
        file.write_all(response_str.as_bytes()).unwrap();
    }
    */

    let start = Instant::now();
    let filtered = filter_tx(&response)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let duration = start.elapsed();
    println!("filter: {:?}", duration);

    /*
    {
        let mut file = File::create(format!("block_filtered.json")).unwrap();
        let filtered_str = filtered.to_string();
        file.write_all(filtered_str.as_bytes()).unwrap();
    }
    */

    Ok((StatusCode::OK, Json(filtered)))
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "Healthy")
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let app = Router::new()
        .route("/", post(get_block))
        .route("/health", get(health_check));

    let listener = tokio::net::TcpListener::bind((args.address, args.port))
        .await
        .expect("bind failed");
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.expect("server failed");
}

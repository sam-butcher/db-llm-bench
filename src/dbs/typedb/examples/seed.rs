//! Seeds the TypeDB `bench` database from data/typedb: drops the database
//! if it exists, recreates it, defines the schema, and inserts the data.
//!
//! Usage (from the repo root, with databases/docker-compose.yml up):
//!   cargo run -p db-typedb --example seed

use futures::StreamExt;
use typedb_driver::answer::QueryAnswer;
use typedb_driver::{
    Addresses, Credentials, DriverOptions, DriverTlsConfig, Transaction, TransactionType,
    TypeDBDriver,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = env_or("TYPEDB_ADDRESS", "localhost:1729");
    let database = env_or("TYPEDB_DATABASE", "bench");
    let username = env_or("TYPEDB_USERNAME", "admin");
    let password = env_or("TYPEDB_PASSWORD", "password");
    let schema = std::fs::read_to_string("data/typedb/schema.tql")?;
    let data = std::fs::read_to_string("data/typedb/data.tql")?;

    let driver = TypeDBDriver::new(
        Addresses::try_from_address_str(&address)?,
        Credentials::new(&username, &password),
        DriverOptions::new(DriverTlsConfig::disabled()),
    )
    .await?;

    if driver.databases().contains(&database).await? {
        driver.databases().get(&database).await?.delete().await?;
        println!("dropped existing database `{database}`");
    }
    driver.databases().create(&database).await?;

    let transaction = driver
        .transaction(&database, TransactionType::Schema)
        .await?;
    run(&transaction, &schema).await?;
    transaction.commit().await?;
    println!("defined schema");

    let transaction = driver.transaction(&database, TransactionType::Write).await?;
    run(&transaction, &data).await?;
    transaction.commit().await?;
    println!("inserted data; `{database}` is seeded at {address}");
    Ok(())
}

async fn run(
    transaction: &Transaction,
    query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Drain row answers so lazily-evaluated inserts complete before commit.
    if let QueryAnswer::ConceptRowStream(_, mut rows) = transaction.query(query).await? {
        while let Some(row) = rows.next().await {
            row?;
        }
    }
    Ok(())
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

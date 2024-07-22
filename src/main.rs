use dotenv::dotenv;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio_postgres::Client;
use tokio_postgres::NoTls;

struct WizLight {
    host_id: String,
    name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let db_host = env::var("DB_HOST").expect("DB_HOST not set");
    let db_user = env::var("DB_USER").expect("DB_USER not set");
    let db_password = env::var("DB_PASSWORD").expect("DB_PASSWORD not set");
    let db_name: String = env::var("DB_NAME").expect("DB_NAME not set");

    let conn_str = format!(
        "host={} user={} password={} dbname={}",
        db_host, db_user, db_password, db_name
    );
    let (client, connection) = tokio_postgres::connect(&conn_str, NoTls).await?;

    // Spawn the connection to run in the background
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let wiz_lights = fetch_wiz_lights(&client).await?;

    // Dim the lights
    let payload_dim = r#"{"method":"setPilot","params":{"state":true,"dimming":10}}"#;
    for light in &wiz_lights {
        match send_udp_packet(&light.host_id, payload_dim).await {
            Ok(_) => {
                let severity = "Info";
                let message = format!("Light {} at {} dimmed to 10%.", light.name, light.host_id);
                println!("SUCCESS: {}", message);
                log_light_event(&client, severity, &message, &light.name).await?;
            }
            Err(e) => {
                let severity: &str = "Error";
                let message = format!(
                    "Failed to dim light {} at {}: {}",
                    light.name, light.host_id, e
                );
                println!("ERROR: {}", message);
                log_light_event(&client, severity, &message, &light.name).await?;
            }
        }
    }

    Ok(())
}

async fn fetch_wiz_lights(client: &Client) -> Result<Vec<WizLight>, Box<dyn std::error::Error>> {
    let rows = client
        .query(
            "SELECT host_id, name FROM machine WHERE set_at_bedtime = TRUE",
            &[],
        )
        .await?;

    let network_id: String = env::var("NETWORK_ID").expect("NETWORK_ID not set");

    let mut wiz_lights = Vec::new();
    for row in rows {
        let host_id: String = row.get("host_id");
        let name: String = row.get("name");
        wiz_lights.push(WizLight {
            host_id: format!("{}.{}:38899", network_id, host_id),
            name,
        });
    }

    Ok(wiz_lights)
}

async fn send_udp_packet(addr: &str, payload: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let addr: SocketAddr = addr.parse()?;
    socket.send_to(payload.as_bytes(), &addr).await?;
    Ok(())
}

async fn log_light_event(
    client: &Client,
    severity: &str,
    message: &str,
    machine: &str,
) -> Result<(), Box<dyn Error>> {
    let event_type = "Bedtime";

    client
        .execute(
            "INSERT INTO log (severity, message, machine, event_type) VALUES ($1, $2, $3, $4)",
            &[&severity, &message, &machine, &event_type],
        )
        .await?;

    Ok(())
}

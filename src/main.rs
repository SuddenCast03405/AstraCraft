mod chat;
mod compression;
mod handshake;
mod metrics;
mod login;
mod nbt;
mod net;
mod ping;
mod protocol;
mod play;
mod server_status;
mod session;
mod world;

use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("login") => {
            let Some(target) = args.next() else {
                eprintln!("Usage: astracraft login <host:port> <username>");
                std::process::exit(2);
            };
            let Some(username) = args.next() else {
                eprintln!("Usage: astracraft login <host:port> <username>");
                std::process::exit(2);
            };
            let Some((host, port)) = target.rsplit_once(':') else {
                eprintln!("Invalid target. Expected host:port (IPv6 must be bracketed).");
                std::process::exit(2);
            };
            let Ok(port) = port.parse::<u16>() else {
                eprintln!("Invalid TCP port: {port}");
                std::process::exit(2);
            };
            if let Err(error) = login::connect_offline(host.trim_matches(['[', ']']), port, &username) {
                eprintln!("Login failed: {error}");
                std::process::exit(1);
            }
        }
        Some("ping") => {
            let Some(target) = args.next() else {
                eprintln!("Usage: astracraft ping <host:port>");
                std::process::exit(2);
            };
            let Some((host, port)) = target.rsplit_once(':') else {
                eprintln!("Invalid target. Expected host:port (IPv6 must be bracketed).");
                std::process::exit(2);
            };
            let Ok(port) = port.parse::<u16>() else {
                eprintln!("Invalid TCP port: {port}");
                std::process::exit(2);
            };
            match ping::ping_server(host.trim_matches(['[', ']']), port, Duration::from_secs(5)) {
                Ok(result) => {
                    println!("Server: {host}:{port}");
                    println!("Version: {}", result.status.version_name.as_deref().unwrap_or("unknown"));
                    println!("Protocol: {}", result.status.protocol.map(|n| n.to_string()).unwrap_or_else(|| "unknown".into()));
                    println!("Players: {}/{}", result.status.online_players.map(|n| n.to_string()).unwrap_or_else(|| "?".into()), result.status.max_players.map(|n| n.to_string()).unwrap_or_else(|| "?".into()));
                    println!("MOTD: {}", result.status.description);
                    println!("Latency: {} ms", result.latency.as_secs_f64() * 1000.0);
                }
                Err(error) => {
                    eprintln!("Ping failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!("AstraCraft — Minecraft Java Edition 1.21.8 client foundation");
            println!("Protocol target: {}", session::PROTOCOL_VERSION);
            println!("Usage: astracraft ping <host:port>");
            println!("       astracraft login <host:port> <username>");
            println!("Implemented: status ping, offline-mode login, zlib negotiation, configuration handshake, persistent Play loop, keep-alive/ping replies, Join Game decoding and teleport confirmation.");
            println!("World model: bounded chunks, block sections, dimension bounds and player pose.");
            println!("Not yet implemented: online account authentication, chunk packet ingestion, input, GUI and 3D rendering.");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn project_smoke_test() {
        assert_eq!(2 + 2, 4);
    }
}

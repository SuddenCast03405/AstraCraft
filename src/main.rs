mod chat;
mod metrics;
mod nbt;
mod protocol;
mod session;

fn main() {
    println!("AstraCraft — Minecraft Java Edition 1.21.8 client foundation");
    println!("Protocol target: {}", session::PROTOCOL_VERSION);
    println!("Status: protocol primitives and parsers; networking/rendering are not wired yet.");
}

#[cfg(test)]
mod tests {
    #[test]
    fn project_smoke_test() {
        assert_eq!(2 + 2, 4);
    }
}

use std::{
    net::TcpListener,
    sync::{atomic::AtomicBool, Arc},
};
fn main() {
    let mut args = std::env::args().skip(1);
    let mut listen = "0.0.0.0:27960".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => {
                listen = args.next().unwrap_or_else(|| {
                    eprintln!("--listen needs ADDR:PORT");
                    std::process::exit(2)
                });
            }
            "--help" | "-h" => {
                println!("codwawzpc-server [--listen ADDR:PORT]\nDefault: 0.0.0.0:27960 (TCP)");
                return;
            }
            _ => {
                eprintln!("unknown argument: {arg}");
                std::process::exit(2);
            }
        }
    }
    let listener = TcpListener::bind(&listen).unwrap_or_else(|error| {
        eprintln!("cannot listen on {listen}: {error}");
        std::process::exit(1)
    });
    eprintln!(
        "codwawzpc relay listening on {} (protocol {})",
        listener.local_addr().unwrap(),
        codwawzpc_coop_proto::VERSION
    );
    codwawzpc_server::serve(listener, Arc::new(AtomicBool::new(false)));
}

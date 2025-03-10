use std::env::args;

#[tokio::main]
async fn main() {
    let Some(path) = args().nth(1) else {
        println!("usage: hath <config>");
        return
    };

    let config = match hath::Config::from_file(&path) {
        Ok(config) => config,
        Err(e) => {
            println!("failed to load config: {}", e);
            return
        }
    };

    if let Err(e) = hath::main(config).await {
        println!("stopped unexpectedly: {:?}", e);
    }
}

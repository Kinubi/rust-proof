use rust_proof::node::Node;

// ============================================================================
// TODO: Chapter 3 - Booting the Node
// 1. Add the `#[tokio::main]` macro to make this an async main function.
// 2. Create a new `Node` instance.
// 3. Spawn the node's `run` loop in a background task using `tokio::spawn`.
// 4. Add an infinite loop with a sleep to keep the main thread alive.
// ============================================================================
#[tokio::main]
async fn main() {
    println!("Starting rust-proof node...");
    let storage = Box::new(rust_proof::storage::SledStorage::new("data").unwrap());
    let (node, _sender) = Node::new(storage);
    tokio::spawn(async move {
        node.run().await;
    });

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

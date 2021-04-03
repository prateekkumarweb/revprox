use tokio::net::{TcpListener, TcpStream};

async fn handle_client(mut client_stream: TcpStream) -> std::io::Result<()> {
    let mut server_stream = TcpStream::connect("127.0.0.1:8000").await?;

    let (mut client_reader, mut client_writer) = client_stream.split();
    let (mut server_reader, mut server_writer) = server_stream.split();

    let client_to_server = tokio::io::copy(&mut client_reader, &mut server_writer);
    let server_to_client = tokio::io::copy(&mut server_reader, &mut client_writer);

    let (client_to_server_result, server_to_client_result) =
        tokio::join!(client_to_server, server_to_client);
    client_to_server_result?;
    server_to_client_result?;

    Ok(())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9000").await?;
    println!("Listening on port 9000");
    loop {
        println!("Waiting for client");
        let (client_stream, addr) = listener.accept().await?;
        println!("Accepted client {:?}", addr);
        handle_client(client_stream).await?;
    }
}

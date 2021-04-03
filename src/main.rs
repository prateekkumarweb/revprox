use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};

fn handle_client(mut client_stream: TcpStream) -> std::io::Result<()> {
    let mut server_stream = TcpStream::connect("127.0.0.1:8000")?;

    let mut buf = [0; 4096];
    let n = client_stream.read(&mut buf)?;
    // println!("Read from client {:?}", &buf[..n]);
    server_stream.write_all(&mut buf[..n])?;

    while let Ok(n) = server_stream.read(&mut buf) {
        if n != 0 {
            // println!("Read from server {:?}", &buf[..n]);
            client_stream.write(&mut buf[..n])?;
        } else {
            break;
        }
    }

    Ok(())
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9000")?;

    for client_stream in listener.incoming() {
        handle_client(client_stream?)?;
    }

    Ok(())
}

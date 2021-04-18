use tokio::io::{self, AsyncRead, AsyncWrite};

pub async fn copy_duplex<T1, T2>(stream_a: T1, stream_b: T2) -> io::Result<()>
where
    T1: AsyncRead + AsyncWrite,
    T2: AsyncRead + AsyncWrite,
{
    let (mut stream_a_reader, mut stream_a_writer) = tokio::io::split(stream_a);
    let (mut stream_b_reader, mut stream_b_writer) = tokio::io::split(stream_b);
    let stream_a_to_b = tokio::io::copy(&mut stream_a_reader, &mut stream_b_writer);
    let stream_b_to_a = tokio::io::copy(&mut stream_b_reader, &mut stream_a_writer);
    tokio::try_join!(stream_a_to_b, stream_b_to_a)?;
    Ok(())
}

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Instant;

use ed25519_dalek::SigningKey;

use tos_proto::messages::{
    Ack, Batch, Hello, HelloAck, Message, SchemaConfirm, SchemaDiff, SchemaOffer, StreamEnd,
    StreamStart, PROTOCOL_VERSION,
};

const REAL_PORT: u16 = 38767;

fn hexdump(prefix: &str, bytes: &[u8]) {
    let cols = 16;
    let total = bytes.len();
    println!("\n┌───[{}]─── {} bytes", prefix, total);
    for (i, chunk) in bytes.chunks(cols).enumerate() {
        let off = i * cols;
        print!("│ {:04x}  ", off);
        for (j, b) in chunk.iter().enumerate() {
            print!("{:02x} ", b);
            if j == cols / 2 - 1 {
                print!(" ");
            }
        }
        for _ in chunk.len()..cols {
            print!("   ");
        }
        print!(" │");
        for b in chunk {
            let c = *b;
            if (32..127).contains(&c) {
                print!("{}", c as char);
            } else {
                print!("·");
            }
        }
        println!("│");
    }
    println!("└{}", "─".repeat(76));
}

fn describe(m: &Message) -> String {
    match m {
        Message::Hello(h) => format!(
            "Hello       v={} node_id={} pk={} encrypt={} caps={:?}",
            h.version,
            hex::encode(&h.node_id[..6]),
            hex::encode(&h.public_key[..6]),
            h.encrypt,
            h.caps
        ),
        Message::HelloAck(a) => format!(
            "HelloAck    v={} node_id={} pk={} caps={:?}",
            a.version,
            hex::encode(&a.node_id[..6]),
            hex::encode(&a.public_key[..6]),
            a.caps
        ),
        Message::SchemaOffer(o) => format!(
            "SchemaOffer sdl={}B hash={} sig={}B",
            o.sdl.len(),
            hex::encode(&o.schema_hash[..6]),
            o.signature.len()
        ),
        Message::SchemaDiff(d) => format!(
            "SchemaDiff  accepted={} reason={:?}",
            d.accepted, d.reason
        ),
        Message::SchemaConfirm(_) => "SchemaConfirm (server ack)".to_string(),
        Message::StreamStart(s) => format!(
            "StreamStart session={} table={:?} mode={} batch_size={}",
            hex::encode(&s.session_id[..6]),
            s.table,
            s.mode,
            s.batch_size
        ),
        Message::Batch(b) => format!(
            "Batch       id={} count={} records={}B hash={} sig={}B",
            b.batch_id,
            b.count,
            b.records.len(),
            hex::encode(&b.batch_hash[..6]),
            b.signature.len()
        ),
        Message::Ack(a) => format!("Ack         batch_id={}", a.batch_id),
        Message::StreamEnd(s) => format!(
            "StreamEnd   session={} total={} duration={}ms",
            hex::encode(&s.session_id[..6]),
            s.total_records,
            s.duration_ms
        ),
        Message::Done(d) => format!(
            "Done        session={} total={} duration={}ms",
            hex::encode(&d.session_id[..6]),
            d.total_records,
            d.duration_ms
        ),
    }
}

fn frame_read<R: Read>(r: &mut R) -> Result<Message, String> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).map_err(|e| e.to_string())?;
    let msg: Message = bincode::deserialize(&payload).map_err(|e| e.to_string())?;
    Ok(msg)
}

fn frame_write_with_log<W: Write>(
    w: &mut W,
    msg: &Message,
    direction: &str,
    log: &Sender<LogEntry>,
) -> Result<(), String> {
    let payload = bincode::serialize(msg).map_err(|e| e.to_string())?;
    let mut framed = Vec::new();
    framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    framed.extend_from_slice(&payload);
    w.write_all(&framed).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    log.send(LogEntry {
        dir: direction.to_string(),
        bytes: framed,
        decoded: describe(msg),
    })
    .ok();
    Ok(())
}

#[derive(Debug)]
struct LogEntry {
    dir: String,
    bytes: Vec<u8>,
    decoded: String,
}

fn server_thread(listener: TcpListener, log: Sender<LogEntry>) {
    let (mut stream, peer) = listener.accept().unwrap();
    println!("\x1b[32m┌──[SERVER] accepted {peer}\x1b[0m");

    let mut csprng = rand::rngs::OsRng;
    let key = SigningKey::generate(&mut csprng);
    let pk = key.verifying_key().to_bytes();
    let server_hello = Hello {
        version: PROTOCOL_VERSION,
        node_id: pk,
        public_key: pk,
        encrypt: false,
        caps: vec!["json".into(), "sqlite".into()],
    };

    let client_hello = match frame_read(&mut stream) {
        Ok(Message::Hello(h)) => h,
        Ok(other) => {
            eprintln!("[server] expected Hello, got {other:?}");
            return;
        }
        Err(e) => {
            eprintln!("[server] read Hello error: {e}");
            return;
        }
    };
    println!("\x1b[32m│\x1b[0m [server] received Hello from {}", hex::encode(&client_hello.node_id[..6]));

    let ack = HelloAck {
        version: PROTOCOL_VERSION,
        node_id: server_hello.node_id,
        public_key: server_hello.public_key,
        x25519_pub: None,
        caps: server_hello.caps.clone(),
    };
    frame_write_with_log(&mut stream, &Message::HelloAck(ack), "S→C", &log).unwrap();
    println!("\x1b[32m│\x1b[0m [server] sent HelloAck node_id={}", hex::encode(&server_hello.node_id[..6]));

    let offer = match frame_read(&mut stream) {
        Ok(Message::SchemaOffer(o)) => o,
        _ => return,
    };
    println!("\x1b[32m│\x1b[0m [server] received SchemaOffer ({}B SDL)", offer.sdl.len());

    let diff = SchemaDiff { accepted: true, reason: None };
    frame_write_with_log(&mut stream, &Message::SchemaDiff(diff), "S→C", &log).unwrap();
    println!("\x1b[32m│\x1b[0m [server] sent SchemaDiff accepted=true");

    frame_write_with_log(&mut stream, &Message::SchemaConfirm(SchemaConfirm), "S→C", &log).unwrap();
    println!("\x1b[32m│\x1b[0m [server] sent SchemaConfirm");

    let start = match frame_read(&mut stream) {
        Ok(Message::StreamStart(s)) => s,
        _ => return,
    };
    println!("\x1b[32m│\x1b[0m [server] received StreamStart table={:?} batch_size={}", start.table, start.batch_size);

    for i in 0..3u32 {
        let recs = (0..3)
            .map(|j| {
                format!(
                    r#"{{"id":{},"name":"u{}","email":"u{}@x.io","age":{},"active":{}}}"#,
                    i * 3 + j,
                    i * 3 + j,
                    i * 3 + j,
                    20 + j,
                    j % 2 == 0
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let rec_bytes = recs.into_bytes();
        let batch = Batch {
            batch_id: i,
            records: rec_bytes.clone(),
            batch_hash: blake3::hash(&rec_bytes).into(),
            signature: vec![0u8; 64],
            count: 3,
        };
        frame_write_with_log(&mut stream, &Message::Batch(batch), "S→C", &log).unwrap();
        println!("\x1b[32m│\x1b[0m [server] sent Batch #{} (3 records)", i);

        let ack = match frame_read(&mut stream) {
            Ok(Message::Ack(a)) => a,
            _ => return,
        };
        println!("\x1b[32m│\x1b[0m [server] received Ack batch_id={}", ack.batch_id);
    }

    let end = StreamEnd {
        session_id: start.session_id,
        total_records: 9,
        duration_ms: 12,
    };
    frame_write_with_log(&mut stream, &Message::StreamEnd(end), "S→C", &log).unwrap();
    println!("\x1b[32m│\x1b[0m [server] sent StreamEnd (9 records, 12ms)");

    let done = match frame_read(&mut stream) {
        Ok(Message::Done(_)) => "Done",
        _ => "?",
    };
    println!("\x1b[32m│\x1b[0m [server] received {done}, closing");
    let _ = stream.shutdown(std::net::Shutdown::Both);
    println!("\x1b[32m└──[SERVER] closed\x1b[0m");
}

fn client_thread(log: Sender<LogEntry>) {
    thread::sleep(std::time::Duration::from_millis(50));
    let mut stream = TcpStream::connect(("127.0.0.1", REAL_PORT)).unwrap();
    println!("\x1b[36m┌──[CLIENT] connected to 127.0.0.1:{REAL_PORT}\x1b[0m");

    let mut csprng = rand::rngs::OsRng;
    let key = SigningKey::generate(&mut csprng);
    let pk = key.verifying_key().to_bytes();

    let hello = Hello {
        version: PROTOCOL_VERSION,
        node_id: pk,
        public_key: pk,
        encrypt: false,
        caps: vec!["postgres".into(), "redis".into(), "json".into()],
    };
    frame_write_with_log(&mut stream, &Message::Hello(hello.clone()), "C→S", &log).unwrap();
    println!("\x1b[36m│\x1b[0m [client] sent Hello node_id={}", hex::encode(&hello.node_id[..6]));

    let ack = match frame_read(&mut stream) {
        Ok(Message::HelloAck(a)) => a,
        _ => return,
    };
    println!("\x1b[36m│\x1b[0m [client] received HelloAck node_id={}", hex::encode(&ack.node_id[..6]));

    let sdl = br#"[schema.users]
id = { type = "int64", primary = true }
name = { type = "text" }
email = { type = "text" }
age = { type = "int64" }
active = { type = "bool" }
"#
    .to_vec();
    let sdl_hash: [u8; 32] = blake3::hash(&sdl).into();
    let offer = SchemaOffer {
        sdl: sdl.clone(),
        schema_hash: sdl_hash,
        signature: vec![0u8; 64],
    };
    frame_write_with_log(&mut stream, &Message::SchemaOffer(offer), "C→S", &log).unwrap();
    println!("\x1b[36m│\x1b[0m [client] sent SchemaOffer ({}B SDL)", sdl.len());

    let diff = match frame_read(&mut stream) {
        Ok(Message::SchemaDiff(d)) => d,
        _ => return,
    };
    println!("\x1b[36m│\x1b[0m [client] received SchemaDiff accepted={}", diff.accepted);

    let confirm = match frame_read(&mut stream) {
        Ok(Message::SchemaConfirm(_)) => "SchemaConfirm",
        _ => return,
    };
    println!("\x1b[36m│\x1b[0m [client] received {confirm}");

    let session_id: [u8; 32] = blake3::hash(b"demo-session-v1").into();
    let start = StreamStart {
        session_id,
        table: "users".into(),
        mode: 0,
        batch_size: 3,
    };
    frame_write_with_log(&mut stream, &Message::StreamStart(start), "C→S", &log).unwrap();
    println!("\x1b[36m│\x1b[0m [client] sent StreamStart");

    for _i in 0..3u32 {
        let batch = match frame_read(&mut stream) {
            Ok(Message::Batch(b)) => b,
            _ => return,
        };
        println!(
            "\x1b[36m│\x1b[0m [client] received Batch #{} ({} records, {}B, hash={})",
            batch.batch_id,
            batch.count,
            batch.records.len(),
            hex::encode(&batch.batch_hash[..6])
        );
        for line in std::str::from_utf8(&batch.records).unwrap().lines() {
            println!("\x1b[36m│\x1b[0m           ▸ {line}");
        }
        let ack = Ack { batch_id: batch.batch_id };
        frame_write_with_log(&mut stream, &Message::Ack(ack), "C→S", &log).unwrap();
        println!("\x1b[36m│\x1b[0m [client] sent Ack batch_id={}", batch.batch_id);
    }

    let end = match frame_read(&mut stream) {
        Ok(Message::StreamEnd(e)) => e,
        _ => return,
    };
    println!("\x1b[36m│\x1b[0m [client] received StreamEnd total={} duration={}ms", end.total_records, end.duration_ms);

    let done = tos_proto::messages::Done {
        session_id,
        total_records: 9,
        duration_ms: 12,
    };
    frame_write_with_log(&mut stream, &Message::Done(done), "C→S", &log).unwrap();
    println!("\x1b[36m│\x1b[0m [client] sent Done, closing");
    let _ = stream.shutdown(std::net::Shutdown::Both);
    println!("\x1b[36m└──[CLIENT] closed\x1b[0m");
}

fn main() {
    let (tx, rx): (Sender<LogEntry>, Receiver<LogEntry>) = channel();

    let listener = TcpListener::bind(("127.0.0.1", REAL_PORT)).unwrap();
    println!("\x1b[1;33m═══ ToS Wire Protocol · Live Capture ═══\x1b[0m");
    println!("listening on 127.0.0.1:{REAL_PORT}");

    let tx_s = tx.clone();
    let server = thread::spawn(move || {
        server_thread(listener, tx_s);
    });
    let tx_c = tx.clone();
    let client = thread::spawn(move || {
        client_thread(tx_c);
    });

    let start = Instant::now();
    client.join().unwrap();
    server.join().unwrap();
    let elapsed = start.elapsed();

    drop(tx);
    let entries: Vec<LogEntry> = rx.iter().collect();

    println!("\n\n{}", "═".repeat(78));
    println!("   WIRE CAPTURE · {} frames · {:.2?}", entries.len(), elapsed);
    println!("{}\n", "═".repeat(78));

    let total_bytes: usize = entries.iter().map(|e| e.bytes.len()).sum();
    for (i, e) in entries.iter().enumerate() {
        let arrow = match e.dir.as_str() {
            "C→S" => "\x1b[36m━━━ CLIENT ──▶ SERVER ━━━\x1b[0m",
            "S→C" => "\x1b[32m━━━ SERVER ──▶ CLIENT ━━━\x1b[0m",
            other => other,
        };
        println!(
            "\n\x1b[1;36m┏━━ frame #{:02} · {} · {} bytes  {}\x1b[0m",
            i + 1,
            e.dir,
            e.bytes.len(),
            arrow
        );
        println!("\x1b[1;33m┃ DECODED\x1b[0m ┃ \x1b[33m{}\x1b[0m", e.decoded);
        hexdump(&format!("frame #{:02}", i + 1), &e.bytes);
    }

    println!(
        "\n\x1b[1;32m✓ total: {} frames, {} bytes on the wire in {:.2?}\x1b[0m",
        entries.len(),
        total_bytes,
        elapsed
    );
}

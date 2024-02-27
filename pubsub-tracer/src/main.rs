use libp2p::{
  core::{upgrade, Multiaddr, PeerId, PublicKey},
  noise::{Keypair, NoiseConfig, X25519Spec},
  swarm::{SwarmBuilder, NegotiatedSubstream},
  tcp::TokioTcpConfig,
  yamux::YamuxConfig,
};
use libp2p_pubsub::{pb, Pubsub};
use std::{
  io::{self, BufReader, BufWriter},
  path::Path,
  sync::{Arc, Mutex},
  time::Duration,
};
use flate2::{bufread::GzDecoder, write::GzEncoder, Compression};
use prost::Message;
use tempfile::NamedTempFile;
use tokio::{
  fs::{self, File},
  runtime::Runtime,
  sync::mpsc::{self, Sender},
  time::sleep,
  io::{AsyncReadExt, AsyncWriteExt},
};

const MAX_LOG_SIZE: u64 = 1024 * 1024 * 64;
const MAX_LOG_TIME: Duration = Duration::from_secs(60 * 60);

pub struct TraceCollector {
  swarm: Arc<Mutex<Option<Swarm<Pubsub>>>>,
  dir: String,
  json_trace: String,
  buf: Arc<Mutex<Vec<pb::trace_event::TraceEvent>>>,
  flush_file_tx: Sender<()>,
  stop_rx: Arc<Mutex<Option<Sender<()>>>>,
}

impl TraceCollector {
  pub fn new(local_key: Keypair<X25519Spec>, dir: String, json_trace: String) -> Result<Self, io::Error> {
    fs::create_dir_all(&dir).await?;
    let local_peer_id = PublicKey::from(local_key.public()).into_peer_id();
    let transport = TokioTcpConfig::new().nodelay(true).upgrade(upgrade::Version::V1).authenticate(NoiseConfig::xx(local_key)).multiplex(YamuxConfig::default()).boxed();
    let mut swarm = {
      let mut pubsub = Pubsub::new(local_peer_id.clone());
      pubsub.set_protocol_handler(Box::new(Self::handle_stream));
      SwarmBuilder::new(transport, pubsub, local_peer_id).executor(Box::new(|fut| {
        tokio::spawn(fut);
      })).build()
    };
    Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/4001".parse()?)?;
    let swarm = Arc::new(Mutex::new(Some(swarm)));
    let buf = Arc::new(Mutex::new(Vec::new()));
    let (flush_file_tx, flush_file_rx) = mpsc::channel(1);
    let (stop_tx, stop_rx) = mpsc::channel(1);
    let trace_collector = TraceCollector {
      swarm,
      dir,
      json_trace,
      buf: buf.clone(),
      flush_file_tx,
      stop_rx: Arc::new(Mutex::new(Some(stop_tx))),
    };
    let tc_ref = &trace_collector;
    tokio::spawn(async move {
      tc_ref.collect_worker(flush_file_rx).await;
    });
    let tc_ref = &trace_collector;
    tokio::spawn(async move {
      tc_ref.monitor_worker().await;
    });
    Ok(trace_collector)
  }

  pub fn stop(&self) {
    let mut stop_tx = self.stop_rx.lock().unwrap().take().unwrap();
    stop_tx.try_send(()).unwrap();
  }

  pub fn flush(&self) {
    self.flush_file_tx.try_send(()).unwrap();
  }

  fn handle_stream(
    swarm: Swarm<Pubsub>,
    mut stream: NegotiatedSubstream,
    buf: Arc<Mutex<Vec<pb::trace_event::TraceEvent>>>,
  ) {
    tokio::spawn(async move {
      let mut gzip_r = GzDecoder::new(BufReader::new(stream));
      let mut msg = pb::TraceEventBatch::default();

      while let Ok(_) = msg.decode_length_delimited(&mut gzip_r) {
        let mut buf = buf.lock().unwrap();
        buf.extend(msg.batch.drain(..));
      }
    });
  }

  async fn collect_worker(&self, mut flush_file_rx: mpsc::Receiver<()>) {
    loop {
      let current = format!("{}/current", self.dir);
      let out = File::create(&current).await?;

      if let Err(err) = self.write_file(out).await {
        panic!("Error writing file: {}", err);
      }

      let base = format!("trace.{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
      let next = format!("{}/{}.pb.gz", self.dir, base);
      fs::rename(current, &next).await?;

      if !self.json_trace.is_empty() {
        self.write_json_trace(&next, &base).await;
      }

      if flush_file_rx.try_recv().is_ok() {
        break;
      }
    }
  }

  async fn write_file(&self, out: File) -> Result<(), io::Error> {
    let mut gzip_w = GzEncoder::new(BufWriter::new(out), Compression::default());
    let mut buf = Vec::new();

    while {
      {
        let mut tmp = self.buf.lock().unwrap();
        std::mem::swap(&mut buf, &mut tmp);
      }

      for evt in buf.iter() {
        evt.encode_length_delimited(&mut gzip_w)?;
      }

      buf.clear();

      tokio::select! {
                _ = sleep(Duration::from_secs(1)) => true,
                _ = self.flush_file_tx.recv() => false,
            }
    } {
      // loop condition
    }

    gzip_w.finish().await?;

    Ok(())
  }

  async fn monitor_worker(&self) {
    let current = format!("{}/current", self.dir);

    loop {
      let start = std::time::Instant::now();
      while start.elapsed() <= MAX_LOG_TIME {
        sleep(Duration::from_secs(60)).await;

        if let Ok(finfo) = fs::metadata(&current).await {
          if finfo.len() > MAX_LOG_SIZE {
            self.flush();
            break;
          }
        } else {
          eprintln!("Error stating trace log file");
        }
      }
    }
  }

  async fn write_json_trace(&self, trace: &str, name: &str) {
    let in_file = File::open(trace).await?;
    let gzip_r = GzDecoder::new(BufReader::new(in_file));
    let tmp_trace = format!("/tmp/{}.json", name);
    let mut out = File::create(&tmp_trace).await?;

    let mut evt = pb::TraceEvent::default();
    let mut pbr = LengthDelimitedCodec::new(gzip_r);

    while let Ok(_) = evt.decode_length_delimited(&mut pbr) {
      serde_json::to_writer(&mut out, &evt)?;
    }

    out.flush().await?;

    let json_trace = format!("{}/{}.json", self.json_trace, name);
    fs::rename(tmp_trace, json_trace).await.unwrap();
  }
}

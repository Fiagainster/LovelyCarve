use std::sync::Arc;

use tokio::io::{duplex, split};

use crate::core::{
    evidence::partitions::SectorSource, filesystem::evidence_session::EvidenceSession,
};

use super::*;

struct MemorySource(Vec<u8>);

impl SectorSource for MemorySource {
    fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.0.len());
        let end = start.saturating_add(len).min(self.0.len());
        Ok(self.0[start..end].to_vec())
    }

    fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

fn map(entries: &[(&str, Value)]) -> Value {
    Value::Map(
        entries
            .iter()
            .map(|(key, value)| (Value::from(*key), value.clone()))
            .collect(),
    )
}

fn services(sources: SourceRegistry) -> HostRpcServices {
    HostRpcServices::new(
        sources,
        FileRegistry::default(),
        EvidenceRegistry::default(),
        [],
    )
}

#[test]
fn host_progress_is_forwarded_as_a_typed_event() {
    let observed = Arc::new(Mutex::new(None));
    let observed_for_sink = Arc::clone(&observed);
    let sink: PluginProgressSink = Arc::new(move |event| {
        *observed_for_sink.lock() = Some(event);
    });
    dispatch_host_request(
        "host.progress",
        map(&[
            ("current", Value::from(7)),
            ("total", Value::from(10)),
            ("message", Value::from("scan")),
        ]),
        &services(SourceRegistry::default()),
        "org.test.progress",
        &Some(sink),
    )
    .expect("progress accepted");
    let event = observed.lock().clone().expect("progress event");
    assert_eq!(event.plugin_id, "org.test.progress");
    assert_eq!(event.current, 7);
    assert_eq!(event.total, Some(10));
    assert_eq!(event.message.as_deref(), Some("scan"));
}

#[test]
fn file_materialization_requires_declared_permission() {
    let temporary = tempfile::tempdir().expect("temp directory");
    let image_path = temporary.path().join("image.raw");
    std::fs::write(&image_path, b"abcdef").expect("raw image");
    let image = Arc::new(EvidenceSession::open(&image_path).expect("open image"));
    let files = FileRegistry::default();
    let handle = files
        .register(
            "org.test",
            "image".to_owned(),
            image,
            "diskgap:0x0:0x6".to_owned(),
            "file.bin".to_owned(),
        )
        .expect("file handle");
    let denied = HostRpcServices::new(
        SourceRegistry::default(),
        files.clone(),
        EvidenceRegistry::default(),
        [],
    );
    let params = map(&[("file_id", Value::from(handle.file_id.clone()))]);
    assert!(matches!(
        dispatch_host_request("file.materialize", params.clone(), &denied, "org.test", &None),
        Err(PluginError::Rpc { code, .. }) if code == "PERMISSION_DENIED"
    ));

    let granted = HostRpcServices::new(
        SourceRegistry::default(),
        files,
        EvidenceRegistry::default(),
        [PluginPermission::FileMaterialize],
    );
    let lease = dispatch_host_request("file.materialize", params, &granted, "org.test", &None)
        .expect("materialize");
    assert!(lease.as_map().is_some());
}

#[tokio::test]
async fn nested_source_read_does_not_deadlock_plugin_call() {
    let sources = SourceRegistry::default();
    let memory_source: Arc<dyn SectorSource + Send + Sync> =
        Arc::new(MemorySource(b"abcdef".to_vec()));
    let source = sources.register(Arc::clone(&memory_source), 512);
    let (host_stream, plugin_stream) = duplex(16 * 1024);
    let (host_reader, host_writer) = split(host_stream);
    let (mut plugin_reader, mut plugin_writer) = split(plugin_stream);

    let plugin = tokio::spawn(async move {
        let hello = map(&[
            ("protocol_min", Value::from(1)),
            ("protocol_max", Value::from(1)),
            (
                "plugin",
                map(&[
                    ("id", Value::from("org.lovelycarve.echo")),
                    ("name", Value::from("Echo")),
                    ("version", Value::from("0.1.0")),
                ]),
            ),
            ("capabilities", Value::Array(vec![Value::from("test")])),
        ]);
        write_frame(
            &mut plugin_writer,
            &LppMessage::request(1, "plugin.hello", hello),
        )
        .await
        .expect("send hello");
        assert!(matches!(
            read_frame(&mut plugin_reader)
                .await
                .expect("hello response"),
            LppMessage::Response { id: 1, .. }
        ));

        let LppMessage::Request { id, method, .. } =
            read_frame(&mut plugin_reader).await.expect("host ping")
        else {
            panic!("expected ping request");
        };
        assert_eq!(method, "ping");
        write_frame(
            &mut plugin_writer,
            &LppMessage::request(
                3,
                "source.read",
                map(&[
                    ("source_id", Value::from(source.source_id)),
                    ("offset", Value::from(1)),
                    ("length", Value::from(3)),
                ]),
            ),
        )
        .await
        .expect("request source bytes");
        let LppMessage::Response {
            id: 3,
            result: Some(bytes),
            error: None,
            ..
        } = read_frame(&mut plugin_reader)
            .await
            .expect("source.read response")
        else {
            panic!("expected successful source response");
        };
        write_frame(&mut plugin_writer, &LppMessage::success(id, bytes))
            .await
            .expect("send ping response");
    });

    let client = PluginClient::connect(
        Box::new(host_reader),
        Box::new(host_writer),
        services(sources),
        None,
        None,
        None,
    )
    .await
    .expect("connect plugin");
    let result = timeout(Duration::from_secs(1), client.call("ping", Value::Nil))
        .await
        .expect("nested RPC must not deadlock")
        .expect("ping succeeds");
    assert_eq!(result, Value::Binary(b"bcd".to_vec()));
    plugin.await.expect("plugin task");
}

#[tokio::test]
async fn timed_out_call_sends_cancel_and_clears_pending() {
    let (host_stream, plugin_stream) = duplex(16 * 1024);
    let (host_reader, host_writer) = split(host_stream);
    let (mut plugin_reader, mut plugin_writer) = split(plugin_stream);
    let plugin = tokio::spawn(async move {
        let hello = map(&[
            ("protocol_min", Value::from(1)),
            ("protocol_max", Value::from(1)),
            (
                "plugin",
                map(&[
                    ("id", Value::from("org.test.timeout")),
                    ("name", Value::from("Timeout")),
                    ("version", Value::from("1")),
                ]),
            ),
        ]);
        write_frame(
            &mut plugin_writer,
            &LppMessage::request(1, "plugin.hello", hello),
        )
        .await
        .expect("hello");
        read_frame(&mut plugin_reader).await.expect("host hello");
        let LppMessage::Request { id, .. } =
            read_frame(&mut plugin_reader).await.expect("slow request")
        else {
            panic!("expected request");
        };
        assert!(matches!(
            read_frame(&mut plugin_reader).await.expect("cancel"),
            LppMessage::Cancel { request_id, .. } if request_id == id
        ));
    });
    let client = PluginClient::connect(
        Box::new(host_reader),
        Box::new(host_writer),
        services(SourceRegistry::default()),
        None,
        None,
        None,
    )
    .await
    .expect("connect");
    assert!(matches!(
        client
            .call_with_timeout("slow", Value::Nil, Duration::from_millis(10))
            .await,
        Err(PluginError::Timeout)
    ));
    assert!(client.pending.lock().is_empty());
    plugin.await.expect("plugin task");
}

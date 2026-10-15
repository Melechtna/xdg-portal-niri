//! Live monitor previews.
//!
//! A preview is a throwaway screen cast: we ask the compositor to record a
//! monitor, take the PipeWire node it gives us, and consume it ourselves over a
//! second PipeWire connection.

use anyhow::{Context, Result};
use pipewire as pw;
use pw::properties::properties;
use tracing::{debug, warn};

use crate::config;
use crate::niri::Niri;
use crate::sources::Selection;

/// A decoded, RGBA, possibly-downscaled frame ready for a texture.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Owns a running preview.
pub struct PreviewHandle {
    quit: Option<pw::channel::Sender<()>>,
}

impl PreviewHandle {
    pub fn stop(&mut self) {
        if let Some(quit) = self.quit.take() {
            let _ = quit.send(());
        }
    }
}

impl Drop for PreviewHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Returns as soon as the worker thread is up.
pub fn spawn(
    selection: Selection,
    on_frame: impl Fn(Frame) + Send + 'static,
) -> Result<PreviewHandle> {
    let (quit_tx, quit_rx) = pw::channel::channel::<()>();

    std::thread::Builder::new()
        .name("xdg-portal-niri-preview".into())
        .spawn(move || {
            if let Err(err) = run(selection, on_frame, quit_rx) {
                warn!("preview thread finished: {err:#}");
            }
        })
        .context("spawning the preview thread")?;

    Ok(PreviewHandle {
        quit: Some(quit_tx),
    })
}

/// The preview thread body.
fn run(
    selection: Selection,
    on_frame: impl Fn(Frame) + Send + 'static,
    quit_rx: pw::channel::Receiver<()>,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building the preview runtime")?;

    let cast = runtime.block_on(async {
        let connection = zbus::Connection::session().await?;
        let niri = Niri::new(connection);
        niri.start_cast(
            std::slice::from_ref(&selection),
            config::PREVIEW_CURSOR_HIDDEN,
        )
        .await
    });

    let cast = cast?;

    let Some(node_id) = cast.streams.first().map(|stream| stream.node_id) else {
        anyhow::bail!("preview capture produced no streams");
    };

    let result = consume(node_id, on_frame, quit_rx);

    runtime.block_on(cast.stop());
    result
}

/// Owns the PipeWire side.
fn consume<F>(node_id: u32, on_frame: F, quit_rx: pw::channel::Receiver<()>) -> Result<()>
where
    F: Fn(Frame) + 'static,
{
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None).context("creating a PipeWire main loop")?;
    let weak = mainloop.downgrade();

    let context =
        pw::context::ContextRc::new(&mainloop, None).context("creating a PipeWire context")?;
    let core = context
        .connect_rc(None)
        .context("connecting to the PipeWire daemon")?;

    let stream = pw::stream::StreamBox::new(
        &core,
        "xdg-portal-niri-preview",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .context("creating the preview stream")?;

    let data = PreviewData {
        format: Default::default(),
        on_frame,
    };

    let _listener = stream
        .add_local_listener_with_user_data(data)
        .param_changed(|_, user_data, id, param| {
            let Some(param) = param else {
                return;
            };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = pw::spa::param::format_utils::parse_format(param)
            else {
                return;
            };
            if media_type != pw::spa::param::format::MediaType::Video
                || media_subtype != pw::spa::param::format::MediaSubtype::Raw
            {
                return;
            }
            if let Err(err) = user_data.format.parse(param) {
                warn!("could not parse the preview's negotiated format: {err}");
            }
        })
        .process(|stream, user_data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            if let Some(frame) = decode(&mut buffer, &user_data.format) {
                (user_data.on_frame)(frame);
            }
        })
        .register()
        .context("registering preview stream listeners")?;

    let format_bytes = enum_format_bytes()?;
    let pod = pw::spa::pod::Pod::from_bytes(&format_bytes)
        .context("the serialized EnumFormat is not a valid pod")?;
    let mut params = [pod];
    stream
        .connect(
            pw::spa::utils::Direction::Input,
            Some(node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .context("connecting the preview stream to niri's node")?;

    let _quit = quit_rx.attach(mainloop.loop_(), move |_| {
        if let Some(mainloop) = weak.upgrade() {
            mainloop.quit();
        }
    });

    debug!("preview consuming node {node_id}");

    mainloop.run();
    Ok(())
}

/// Per-stream state handed to the PipeWire callbacks.
struct PreviewData<F> {
    format: pw::spa::param::video::VideoInfoRaw,
    on_frame: F,
}

/// Turn a dequeued buffer into an RGBA frame.
fn decode(
    buffer: &mut pw::buffer::Buffer<'_>,
    format: &pw::spa::param::video::VideoInfoRaw,
) -> Option<Frame> {
    let video_format = format.format();
    let width = format.size().width as usize;
    let height = format.size().height as usize;
    if width == 0 || height == 0 {
        return None;
    }

    let components = match video_format {
        pw::spa::param::video::VideoFormat::BGRx => Channels {
            r: 2,
            g: 1,
            b: 0,
            a: None,
        },
        pw::spa::param::video::VideoFormat::BGRA => Channels {
            r: 2,
            g: 1,
            b: 0,
            a: Some(3),
        },
        pw::spa::param::video::VideoFormat::RGBx => Channels {
            r: 0,
            g: 1,
            b: 2,
            a: None,
        },
        pw::spa::param::video::VideoFormat::RGBA => Channels {
            r: 0,
            g: 1,
            b: 2,
            a: Some(3),
        },
        other => {
            debug!("ignoring unsupported preview format {other:?}");
            return None;
        }
    };

    let datas = buffer.datas_mut();
    let data = datas.first_mut()?;
    let (offset, size, stride) = {
        let chunk = data.chunk();
        (
            chunk.offset() as usize,
            chunk.size() as usize,
            chunk.stride(),
        )
    };
    let bytes = data.data()?;
    let end = offset
        .checked_add(size)
        .unwrap_or(bytes.len())
        .min(bytes.len());
    let src = bytes.get(offset..end)?;

    let stride = if stride > 0 {
        stride as usize
    } else {
        width * 4
    };

    let rgba = to_rgba(src, width, height, stride, components);
    let (width, height, rgba) = downscale(rgba, width, height, config::PREVIEW_MAX_EDGE as usize);
    Some(Frame {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

/// Which byte of a 4-byte pixel holds which channel.
struct Channels {
    r: usize,
    g: usize,
    b: usize,
    a: Option<usize>,
}

fn to_rgba(src: &[u8], width: usize, height: usize, stride: usize, c: Channels) -> Vec<u8> {
    let mut out = vec![0u8; width * height * 4];
    for y in 0..height {
        let row = y * stride;
        for x in 0..width {
            let i = row + x * 4;
            if i + 4 > src.len() {
                return out;
            }
            let o = (y * width + x) * 4;
            out[o] = src[i + c.r];
            out[o + 1] = src[i + c.g];
            out[o + 2] = src[i + c.b];
            out[o + 3] = c.a.map_or(255, |a| src[i + a]);
        }
    }
    out
}

/// Nearest-neighbour downscale so the GTK hop stays cheap.
fn downscale(
    rgba: Vec<u8>,
    width: usize,
    height: usize,
    max_edge: usize,
) -> (usize, usize, Vec<u8>) {
    let longest = width.max(height);
    if longest <= max_edge || max_edge == 0 {
        return (width, height, rgba);
    }

    let factor = longest.div_ceil(max_edge);
    let out_w = width.div_ceil(factor);
    let out_h = height.div_ceil(factor);
    let mut out = vec![0u8; out_w * out_h * 4];
    for y in 0..out_h {
        let src_y = (y * factor).min(height - 1);
        for x in 0..out_w {
            let src_x = (x * factor).min(width - 1);
            let si = (src_y * width + src_x) * 4;
            let di = (y * out_w + x) * 4;
            out[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
        }
    }
    (out_w, out_h, out)
}

/// The `EnumFormat` we advertise, raw video, 4-byte packed, no dmabuf.
fn enum_format_bytes() -> Result<Vec<u8>> {
    use pw::spa::param::format::FormatProperties;
    use pw::spa::param::video::VideoFormat;
    use pw::spa::pod::serialize::PodSerializer;
    use pw::spa::pod::Value as PodValue;

    let object = pw::spa::pod::object!(
        pw::spa::utils::SpaTypes::ObjectParamFormat,
        pw::spa::param::ParamType::EnumFormat,
        pw::spa::pod::property!(
            FormatProperties::MediaType,
            Id,
            pw::spa::param::format::MediaType::Video
        ),
        pw::spa::pod::property!(
            FormatProperties::MediaSubtype,
            Id,
            pw::spa::param::format::MediaSubtype::Raw
        ),
        pw::spa::pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
        ),
    );

    Ok(
        PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &PodValue::Object(object))
            .context("serializing the preview EnumFormat")?
            .0
            .into_inner(),
    )
}

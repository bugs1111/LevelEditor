use super::native_gpu_config;
use eframe::{egui::epaint::mutex::RwLock, egui_wgpu, wgpu};
use std::{future::Future, sync::Arc};

fn block_on<F: Future>(future: F) -> F::Output {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Wake(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}

#[test]
#[ignore = "requires a real GPU device"]
fn surface_error_submits_pending_uploads_without_a_present() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = block_on(adapter.request_device(&Default::default())).unwrap();
    eprintln!("GPU adapter: {:?}", adapter.get_info());
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let state = egui_wgpu::RenderState {
        available_adapters: vec![adapter.clone()],
        adapter,
        device: device.clone(),
        queue: queue.clone(),
        target_format: format,
        renderer: Arc::new(RwLock::new(egui_wgpu::Renderer::new(
            &device,
            format,
            Default::default(),
        ))),
    };
    crate::canvas::configure_gpu(Some(&state));
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pending surface-error upload"),
        size: 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    // Queue writes retain staging allocations until a submit. Polling alone
    // cannot replace it when the painter skips a frame after acquisition fails.
    queue.write_buffer(&output, 0, &[1, 2, 3, 4]);
    let config = native_gpu_config();
    assert!(matches!(
        (config.on_surface_error)(wgpu::SurfaceError::Timeout),
        egui_wgpu::SurfaceErrorAction::SkipFrame
    ));
    let (tx, rx) = std::sync::mpsc::channel();
    output
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(5)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("surface-error uploads must be submitted, even with no present")
        .unwrap();
    assert_eq!(&*output.slice(..).get_mapped_range(), &[1, 2, 3, 4]);
}

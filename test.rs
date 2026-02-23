#[no_mangle]
pub extern "C" fn iris_create_engine(
    hwnd: *mut std::ffi::c_void,
    width: u32,
    height: u32,
) -> *mut IrisEngine {
    let result = panic::catch_unwind(|| {
        // 1. 先只创建基础实例 (Instance)
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12, // 确保使用 DX12
            ..Default::default()
        });

        // 2. 构造 Window Handle
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            raw_window_handle: RawWindowHandle::Win32({
                let mut h = Win32WindowHandle::new(
                    std::num::NonZeroIsize::new(hwnd as isize).expect("HWND 不能为空"),
                );
                h
            }),
        };

        // 3. 关键改变：先创建 Surface！
        let surface = unsafe { instance.create_surface_unsafe(target) }.expect("Surface 创建失败");

        // 4. 请求 Adapter 时，传入 compatible_surface，确保显卡支持这个窗口！
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface), // 告诉系统我们要在这个 surface 上渲染
            force_fallback_adapter: false,
        }))
        .expect("找不到兼容的显卡适配器");

        // 5. 获取 Device 和 Queue
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("MogaIrisDevice"),
                ..Default::default()
            },
            None,
        ))
        .expect("Device 创建失败");

        // 6. 重新获取能力支持。现在有了 compatible_surface 的保证，caps 绝对不会为空了。
        let caps = surface.get_capabilities(&adapter);

        // 更安全的获取方式，避免 [0] 导致的越界崩溃
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or_else(|| {
                caps.formats
                    .first()
                    .copied()
                    .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb)
            });

        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        // 7. 配置 Surface
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1), // 防止 C# 传过来宽高为 0 导致崩溃
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // 8. 手动组装 GpuContext (不再调用 GpuContext::new，以适配新的初始化顺序)
        let context = GpuContext {
            instance,
            adapter,
            device,
            queue,
        };

        let engine = Box::new(IrisEngine {
            context,
            surface,
            config,
        });
        Box::into_raw(engine)
    });

    result.unwrap_or_else(|err| {
        let msg = if let Some(s) = err.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = err.downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let _ = std::fs::write("rust_crash_log.txt", format!("Rust引擎启动崩溃: {}", msg));
        std::ptr::null_mut()
    })
}

mod hardware;

use crate::hardware::instance::GpuContext;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use std::{fs, panic};

pub struct IrisEngine {
    pub context: GpuContext,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

#[no_mangle]
pub extern "C" fn iris_create_engine(
    hwnd: *mut std::ffi::c_void,
    width: u32,
    height: u32,
) -> *mut IrisEngine {
    let result = panic::catch_unwind(|| {
        //1、创建基础实例
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });
        // 2、构建window handle
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            raw_window_handle: RawWindowHandle::Win32(Win32WindowHandle::new(
                std::num::NonZeroIsize::new(hwnd as isize).expect("HWND 不能为空"),
            )),
        };

        //3、 创建surface
        let surface =
            unsafe { instance.create_surface_unsafe(target) }.expect("Failed to create window");
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

        // 6. 获取 Surface 支持的配置
        let caps = surface.get_capabilities(&adapter);
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

        // 【关键优化 1】：寻找 Mailbox 或 Immediate 模式，彻底解除缩放时的 VSync 阻塞
        let present_mode = caps
            .present_modes
            .iter()
            .copied()
            .find(|&m| m == wgpu::PresentMode::Mailbox || m == wgpu::PresentMode::Immediate)
            .unwrap_or(wgpu::PresentMode::AutoNoVsync); // 如果都不支持，强制不等待

        // 7. 配置 Surface
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::AutoNoVsync, // 使用无阻塞模式
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 1, // 【关键优化 2】：将帧积压降到 1，保证缩放时画面绝对最新
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
        // 如果发生 Panic，提取具体的报错信息
        let msg = if let Some(s) = err.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = err.downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        // 把崩溃原因写到当前运行目录下的日志文件里！
        let _ = fs::write("rust_crash_log.txt", format!("Rust引擎启动崩溃: {}", msg));

        // 返回空指针给 C#，而不是让程序闪退
        std::ptr::null_mut()
    })
}

#[no_mangle]
pub extern "C" fn iris_destroy_engine(engine_ptr: *mut IrisEngine) {
    if !engine_ptr.is_null() {
        unsafe {
            drop(Box::from_raw(engine_ptr));
        }
    }
}
#[no_mangle]
pub extern "C" fn iris_resize_engine(engine_ptr: *mut IrisEngine, width: u32, height: u32) {
    if engine_ptr.is_null() {
        return;
    }
    let engine = unsafe { &mut *engine_ptr };
    engine.config.width = width.max(1);
    engine.config.height = height.max(1);
    engine
        .surface
        .configure(&engine.context.device, &engine.config);
}

#[no_mangle]
pub extern "C" fn iris_render_frame(engine_ptr: *mut IrisEngine) {
    // 增加一个空指针保护，防止 C# 端传错导致 Rust 崩溃
    if engine_ptr.is_null() {
        return;
    }
    let engine = unsafe { &mut *engine_ptr };
    let ctx = &engine.context;

    //1、从surface 拿到当前帧可以用来渲染的纹理
    let output = match engine.surface.get_current_texture() {
        Ok(texture) => texture,
        Err(e) => {
            eprintln!("获取surface纹理失败:{:?}", e);
            return;
        }
    };
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    // 2. 开始渲染编码
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Main Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        // TODO: 这里之后调用 ROI 等几何绘制逻辑
    }

    // 3. 提交渲染命令给 GPU
    ctx.queue.submit(std::iter::once(encoder.finish()));

    // 4. 将画面呈现在 HWND 的屏幕上！
    output.present();
}

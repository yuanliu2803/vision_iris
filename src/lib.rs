mod hardware;

use crate::hardware::instance::GpuContext;
use crate::hardware::texture_export::SharedTexture;
use std::{fs, panic};

pub struct IrisEngine {
    pub context: GpuContext,
    pub render_target: SharedTexture,
}

#[no_mangle]
pub extern "C" fn iris_create_engine() -> *mut IrisEngine {
    let result = panic::catch_unwind(|| {
        let context = pollster::block_on(GpuContext::new());
        let render_target = SharedTexture::new(&context.device, 800, 600);
        let engine = Box::new(IrisEngine {
            context,
            render_target,
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
pub extern "C" fn iris_render_frame(engine_ptr: *mut IrisEngine) -> *mut std::ffi::c_void {
    // 增加一个空指针保护，防止 C# 端传错导致 Rust 崩溃
    if engine_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let engine = unsafe { &mut *engine_ptr };
    let ctx = &engine.context;
    let shared_tex = &engine.render_target;

    // 2. 开始渲染编码
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &shared_tex.view,
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
            ..Default::default()
        });
        // TODO: 这里之后调用 ROI 绘制逻辑
    }

    // 1. 提交 WGPU 渲染，并拿到一个任务索引
    let submission_index = ctx.queue.submit(Some(encoder.finish()));

    // 2. 让 CPU 稍微阻塞一下，确保 WGPU 在 GPU 上把画面彻底画完了
    ctx.device
        .poll(wgpu::Maintain::WaitForSubmissionIndex(submission_index));

    // 3. WGPU 画完后，安全地将结果复制到可以与 WPF 共享的纹理里
    shared_tex.sync_to_shared();

    // 4. 终于能把句柄交出去了！
    shared_tex.shared_handle.0 as *mut std::ffi::c_void
}

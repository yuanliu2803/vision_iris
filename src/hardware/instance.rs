use wgpu::{Adapter, Backends, Device, Instance, InstanceDescriptor, Queue};

pub struct GpuContext {
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
}

impl GpuContext {
    pub async fn new() -> Self {
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::DX12, // WPF 在 Windows 上与 DX11/12 兼容性最好
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("MogaIrisDevice"),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        Self {
            instance,
            adapter,
            device,
            queue,
        }
    }
}

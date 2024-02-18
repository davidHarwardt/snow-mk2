
use std::{marker::PhantomData, ops::{Deref, DerefMut}, num::NonZeroU64};

use bytemuck::Pod;
use wgpu::util::DeviceExt;


pub struct UniformBuffer<T>(wgpu::Buffer, T);

impl<T> Deref for UniformBuffer<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &self.1 }
}
impl<T> DerefMut for UniformBuffer<T> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.1 }
}

impl<T: Pod> UniformBuffer<T> {
    pub fn new(device: &wgpu::Device, data: T, label: Option<&str>) -> Self {
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[data])
        });

        Self(buf, data)
    }

    pub fn write(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.0, 0, bytemuck::cast_slice(&[self.1]));
    }

    pub fn buffer(&self) -> &wgpu::Buffer { &self.0 }
    pub fn binding_ty(&self) -> wgpu::BindingType {
        wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: Some(
                NonZeroU64::new(std::mem::size_of::<T>() as _).unwrap(),
            ),
        }
    }
}




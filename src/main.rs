#![allow(unused_variables)]

#[macro_use]
extern crate vulkano_shader_derive;
extern crate vulkano;
extern crate image;

use vulkano::instance::Instance;
use vulkano::instance::InstanceExtensions;
use vulkano::instance::PhysicalDevice;

use vulkano::device::Device;
use vulkano::device::Queue;
use vulkano::device::DeviceExtensions;
use vulkano::instance::Features;

use vulkano::buffer::BufferUsage;
use vulkano::buffer::CpuAccessibleBuffer;

use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::command_buffer::CommandBuffer;
use vulkano::sync::GpuFuture;

use std::sync::Arc;

use vulkano::pipeline::ComputePipeline;
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;

use vulkano::format::Format;
use vulkano::image::Dimensions;
use vulkano::image::StorageImage;

use vulkano::format::ClearValue;

use image::ImageBuffer;
use image::Rgba;

struct MyStruct {
	a: u32,
	b: bool,
}

mod cs {
    #[derive(VulkanoShader)]
    #[ty = "compute"]
    #[src = "
#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Data {
	uint data[];
} buf;

void main() {
	uint idx = gl_GlobalInvocationID.x;
	buf.data[idx] *= 12;
}//"
	]
	struct Dummy;
}

fn main() {
	let (device, queue) = create_vulkan();
	create_buffers(device.clone());
	simple_gpu_copy(device.clone(), queue.clone());
	simple_gpu_shader_compute(device.clone(), queue.clone());

	let image = create_image(device.clone(), queue.clone());
	clear_image(image.clone(), device.clone(), queue.clone());

}

fn create_vulkan() -> (Arc<Device>, Arc<Queue>) {
	let instance = Instance::new(None, &InstanceExtensions::none(), None).expect("failed to create instance");
	let physical = PhysicalDevice::enumerate(&instance).next().expect("no device available");
	for family in physical.queue_families() {
		println!("Found a queue family with {:?} queue(s)", family.queues_count());
	}

	let queue_family = physical.queue_families()
		.find(|&q| q.supports_graphics())
		.expect("couldn't find a graphical queue family");

	let (device, mut queues) = {
	Device::new(physical, &Features::none(), &DeviceExtensions::none(),
		[(queue_family, 0.5)].iter().cloned()).expect("failed to create device")
	};
	let queue = queues.next().unwrap();

	//tip: to figure out something's type, fail compilation on casting : 
	//device as (); //--> std::sync::Arc<vulkano::device::Device>
	//queue as (); //-> Arc<Queue>
	
	(device, queue)
}

fn create_buffers(device: Arc<Device>) {
	//Many kinds of buffers : CpuAccessibleBuffer, ImmutableBuffer, CpuBufferPool
	//Simplest kind of buffer is CpuAccessibleBuffer

	//you can add anything in a buffer using from_data
	let i32_data = 12;
	let buffer_from_data_1 = CpuAccessibleBuffer::from_data(device.clone(), BufferUsage::all(), i32_data).expect("failed to create buffer");

	//You can put any type you want in a buffer
	//note: using a type that doesn't implement the Send, Sync and Copy traits or that isn't 'static will restrict what you can do with that buffer.
	let struct_data = MyStruct { a: 5, b: true };
	let buffer_from_data_2 = CpuAccessibleBuffer::from_data(device.clone(), BufferUsage::all(), struct_data).unwrap();

	//use from_iter to create a buffer from any array 
	let iter = (0 .. 128).map(|_| 5u8);
	let buffer_from_iter = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), iter).unwrap();

	//write to buffers
	let mut content = buffer_from_data_2.write().unwrap();
	//`content` implements `DerefMut` whose target is of type `MyStruct` (the content of the buffer)
	//content as (); //-> vulkano::buffer::cpu_accss::WriteLock<'_, MyStruct>
	content.a *= 2;
	content.b = false;

	let mut array_content = buffer_from_iter.write().unwrap();
	array_content[12] = 83;
}

fn simple_gpu_copy(device: Arc<Device>, queue: Arc<Queue>) {
	//buffer operations basics
	//create src/dst buffers
	let source_content = 0 .. 64;
	let src = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), source_content).expect("failed to create buffer");

	let dest_content = (0 .. 64).map(|_| 0);
	let dst = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), dest_content).expect("failed to create buffer");

	//create a command buffer with copy_buffer command
	//reminder: all the types are Arc<...> so .clone() only copies a pointer 
	let command_buffer = AutoCommandBufferBuilder::new(device.clone(), queue.family()).unwrap()
		.copy_buffer(src.clone(), dst.clone()).unwrap()
		.build().unwrap();

	//submit the command buffer
	let submitted = command_buffer.execute(queue.clone()).unwrap();

	//submitting an operation doesn't wait for completion
	//use method from the 'submitted' object returned by execute
	submitted.then_signal_fence_and_flush().unwrap().wait(None).unwrap();

	let src_content = src.read().unwrap();
	let dst_content = dst.read().unwrap();
	assert_eq!(&*src_content, &*dst_content);
	println!("GPU copy succeeded!");
}

fn simple_gpu_shader_compute(device: Arc<Device>, queue: Arc<Queue>) {
	//compute operations: let's multiply all the values in our buffer by 12
	let data_iter = 0 .. 65536;
	let data_buffer = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), data_iter).expect("failed to create buffer");


	//Shader::load was created by vulkano_shader_derive which compiles the 	GLSL code
	let shader = cs::Shader::load(device.clone()).expect("failed to create shader module");

	//create a compute pipeline which we wil need to execute
	let compute_pipeline = Arc::new(ComputePipeline::new(device.clone(), &shader.main_entry_point(), &()).expect("failed to create compute pipeline"));

	//see GLSL code :
	//layout(set = 0, binding = 0) buffer Data
	//defines the descriptor #0 in set #0.
	//descriptors can contain many things (buffer, buffer view, image, sampled image, ...), here a buffer
	//compute pipeline => descriptor set => descriptor

	//now we will create the descriptor set that will contain a descriptor for our buffer
	let set = Arc::new(PersistentDescriptorSet::start(compute_pipeline.clone(), 0)
	.add_buffer(data_buffer.clone()).unwrap()
	.build().unwrap());

	//now we can create a new command buffer that will execute our compute pipeline
	//see GLSL code :
	//layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;
	//we have 64k elements. We want to aim for a local size of 32/64, that's why we spawn 1024 work groups
	let command_buffer = AutoCommandBufferBuilder::new(device.clone(), queue.family()).unwrap()
		.dispatch([1024, 1, 1], compute_pipeline.clone(), set.clone(), ()).unwrap()
		.build().unwrap();

	let submitted = command_buffer.execute(queue.clone()).unwrap();
	submitted.then_signal_fence_and_flush().unwrap().wait(None).unwrap();

	//read back the results
	let content = data_buffer.read().unwrap();
	for (n, val) in content.iter().enumerate() {
		assert_eq!(*val, n as u32 * 12);
	}

	println!("GPU multiply-by-12 operation successful!");
}

fn create_image(device: Arc<Device>,queue: Arc<Queue>) -> Arc<StorageImage<Format>> {
	//lots of documentation http://vulkano.rs/guide/image-creation
	//image structures are a specialized way to store image data on the GPU

	//we create a StorageImage, which is the general purpose image container 
	let image = StorageImage::new(device.clone(), Dimensions::Dim2d { width: 1024, height: 1024 }, 
		Format::R8G8B8A8Unorm, Some(queue.family())).unwrap();

	image
}

fn clear_image(image: Arc<StorageImage<Format>>, device: Arc<Device>, queue: Arc<Queue>) {
	//images have an opaque implementation-specific memory layout
	//this means you can not modify this image by writing directly in its memory
	//Instead we use specific commands to ask the GPU to do operations

	//create a CpuAccessibleBuffer to store the result and export back to the CPU
	let buf = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(),
	(0 .. 1024 * 1024 * 4).map(|_| 0u8))
	.expect("failed to create buffer");


	//clear_color_image paints the image with a single color
	//format R8G8B8A8Unorm means we use floating point values between 0 and 1.
	//GPU stores that into 8 bits mapping 0.0 to 0 and 1.0 to 255.
	//copy_image_to_buffer copies image to buffer.
	//buffer can then be considered as "real" U8
	let command_buffer = AutoCommandBufferBuilder::new(device.clone(), queue.family()).unwrap()
	.clear_color_image(image.clone(), ClearValue::Float([1.0, 0.0, 1.0, 1.0])).unwrap()
	.copy_image_to_buffer(image.clone(), buf.clone()).unwrap()
	.build().unwrap();

	let finished = command_buffer.execute(queue.clone()).unwrap();
	finished.then_signal_fence_and_flush().unwrap().wait(None).unwrap();

	let buffer_content = buf.read().unwrap();
	let image = ImageBuffer::<Rgba<u8>, _>::from_raw(1024, 1024, &buffer_content[..]).unwrap();
	let path = "image.png";
	image.save(path).unwrap();
	println!("Image saved to {:?}", path);
}
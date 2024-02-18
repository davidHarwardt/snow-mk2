

struct Instance {
    pos: vec2<f32>,
    vel: vec2<f32>,
    scale: f32,
    age: f32,
}

struct ShaderData {
    dt: f32,
    time: f32,
    gravity: vec2<f32>,
    aspect: f32,
    max_age: f32,
}

@group(0) @binding(0)
var<uniform> data: ShaderData;

@group(1) @binding(0)
var<storage, read_write> instances: array<Instance>;


fn rand(co: vec2<f32>) -> f32 {
    return fract(sin(dot(co, vec2(12.9898, 78.233))) * 43758.5453);
}

fn simple_noise(v: f32) -> f32 {
    return sin(v * 0.2) * 0.4
         + sin(v * 0.9) * 0.2
         + sin(v * 5.0) * 0.05
         + sin(v * 0.5) * 0.35;
}

fn rotate(v: vec2<f32>, a: f32) -> vec2<f32> {
    let sin_v = sin(a);
    let cos_v = cos(a);
    return vec2<f32>(v.x * cos_v - v.y * sin_v, v.x * sin_v + v.y * cos_v);
}

@compute
@workgroup_size(256)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
) {
    let padding = 0.1;
    // let padding = -0.1;
    let i = global_id.x;

    var pos = instances[i].pos;
    var vel = instances[i].vel;
    if length(vel) < 0.01 {
        instances[i].age += data.dt;
    }

    let rot = simple_noise(pos.y + pos.x / 10.0 + data.time / 1.0) * 1.0;
    pos += vel * data.dt * 0.9;
    vel += rotate(data.gravity, rot) * data.dt * instances[i].scale;

    if pos.y + padding < -1.0 || instances[i].age > data.max_age {
        instances[i].scale = (rand(pos) * 1.4 + 0.1) * 0.01;
        pos.x = rand(vel) * 2.0 - 1.0;
        pos.y += 2.0 * (1.0 + padding);
        vel = vec2<f32>(0.0);
        instances[i].age = 0.0;
    }

    // also works for all components
    pos.x = (((pos.x + 1.0) / 2.0 + 1.0) % 1.0) * 2.0 - 1.0;


    instances[i].pos = pos;
    instances[i].vel = vel;
}


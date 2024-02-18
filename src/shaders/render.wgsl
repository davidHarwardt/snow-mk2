

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) pos: vec2<f32>,
}

struct VertexInput {
    @location(0) pos: vec2<f32>,
}

struct ShaderData {
    dt: f32,
    time: f32,
    gravity: vec2<f32>,
    aspect: f32,
    max_age: f32,
}

struct InstanceInput {
    @location(10) pos: vec2<f32>,
    @location(11) vel: vec2<f32>,
    @location(12) scale: f32,
    @location(13) age: f32,
}

@group(0) @binding(0)
var<uniform> data: ShaderData;

@vertex
fn vertex_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    out.clip_pos = vec4<f32>(
        vec3<f32>(
            model.pos / vec2<f32>(data.aspect, 1.0) * instance.scale + instance.pos,
            0.0,
        ), 1.0,
    );

    out.pos = model.pos;

    return out;
}


@fragment
fn fragment_main(
    vertex: VertexOutput,
) -> @location(0) vec4<f32> {
    let blend = smoothstep(0.6, 0.5, length(vertex.pos * vec2<f32>(1.0, 1.0)));
    return vec4<f32>(vec3<f32>(1.0), blend);
}


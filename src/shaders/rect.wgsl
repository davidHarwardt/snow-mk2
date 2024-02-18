
struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
}

struct VertexInput {
    @location(0) pos: vec2<f32>,
}

struct InstanceInput {
    @location(10) pos: vec2<f32>,
    @location(11) dim: vec2<f32>,
}

@vertex
fn vertex_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    let pos = ((model.pos + 1.0) / 2.0) * instance.dim + instance.pos;
    let p = vec2<f32>(pos.x, -pos.y) * 4.0 + vec2<f32>(-1.0, 1.0);
    out.clip_pos = vec4<f32>(vec3<f32>(p, 0.0), 1.0);

    return out;
}

@fragment
fn fragment_main(
    vertex: VertexOutput,
) -> @location(0) vec4<f32> {
    return vec4<f32>(vec3<f32>(1.0), 0.05);
}






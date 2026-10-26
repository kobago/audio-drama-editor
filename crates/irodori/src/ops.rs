//! 要素ごとの broadcast 演算など。
//!
//! candle の Metal バックエンドは broadcast を伴う二項演算や軸入れ替えのコピーが遅い
//! (連続テンソル同士の 7〜10 倍)。Metal では専用カーネルを使い、それ以外は candle の演算に任せる。

use candle_core::{D, Result, Tensor};

/// `v` (長さ M) を `x` に broadcast する際の添字: `v[(i / div) % M]`。
/// - 最終軸に沿った broadcast (行ベクトル): div = 1
/// - (B, C, L) のチャンネル方向: div = L
#[derive(Debug, Clone, Copy)]
pub enum Axis {
    Last,
    Channel,
}

impl Axis {
    #[cfg_attr(not(feature = "metal"), allow(dead_code))]
    fn div(self, x: &Tensor) -> Result<usize> {
        Ok(match self {
            Axis::Last => 1,
            Axis::Channel => x.dim(D::Minus1)?,
        })
    }

    fn fallback_shape(self, x: &Tensor, v: &Tensor) -> Result<Tensor> {
        match self {
            Axis::Last => Ok(v.clone()),
            Axis::Channel => v.reshape((v.elem_count(), 1)),
        }
        .and_then(|v| v.broadcast_as(x.shape()))
    }
}

/// x + v
pub fn bcast_add(x: &Tensor, v: &Tensor, axis: Axis) -> Result<Tensor> {
    #[cfg(feature = "metal")]
    if x.device().is_metal() {
        return metal::binary(x, v, axis.div(x)?, "bcast_add_f32");
    }
    x + axis.fallback_shape(x, v)?
}

/// x * v
pub fn bcast_mul(x: &Tensor, v: &Tensor, axis: Axis) -> Result<Tensor> {
    #[cfg(feature = "metal")]
    if x.device().is_metal() {
        return metal::binary(x, v, axis.div(x)?, "bcast_mul_f32");
    }
    x * axis.fallback_shape(x, v)?
}

/// a + v * b (ゲート付き残差)
pub fn bcast_fma(a: &Tensor, v: &Tensor, b: &Tensor, axis: Axis) -> Result<Tensor> {
    #[cfg(feature = "metal")]
    if a.device().is_metal() {
        return metal::fma(a, v, b, axis.div(a)?);
    }
    a + (b * axis.fallback_shape(b, v)?)?
}

/// Snake 活性化 x + sin(alpha x)^2 / (alpha + 1e-9)。x: (B, C, L)、alpha: (C)
pub fn snake(x: &Tensor, alpha: &Tensor) -> Result<Tensor> {
    #[cfg(feature = "metal")]
    if x.device().is_metal() {
        return metal::binary(x, alpha, Axis::Channel.div(x)?, "snake_f32");
    }
    let a = Axis::Channel.fallback_shape(x, alpha)?;
    let inv = (&a + 1e-9)?.recip()?;
    x + (x * &a)?.sin()?.sqr()?.mul(&inv)?
}

/// 最終軸の RMSNorm。`weight` は (G, D) で、行 r には weight[r % G] を掛ける
/// (q_norm / k_norm の (heads, head_dim) 重み)。
pub fn rms_norm_grouped(x: &Tensor, weight: &Tensor, eps: f32) -> Result<Tensor> {
    #[cfg(feature = "metal")]
    if x.device().is_metal() {
        return metal::rms_norm_grouped(x, weight, eps);
    }
    let rms = (x.sqr()?.mean_keepdim(D::Minus1)? + eps as f64)?.sqrt()?;
    x.broadcast_div(&rms)?.broadcast_mul(weight)
}

/// (A, B1, B2, C) → (A, B2, B1, C) の連続テンソル。
pub fn swap12(x: &Tensor) -> Result<Tensor> {
    #[cfg(feature = "metal")]
    if x.device().is_metal() {
        return metal::swap12(x);
    }
    x.transpose(1, 2)?.contiguous()
}

#[cfg(feature = "metal")]
mod metal {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    use candle_core::backend::BackendStorage;
    use candle_core::{
        CpuStorage, CustomOp1, CustomOp2, CustomOp3, Layout, MetalStorage, Result, Shape, Tensor,
        bail,
    };
    use candle_metal_kernels::metal::ComputePipeline;
    use objc2_metal::MTLSize;

    const SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

kernel void bcast_add_f32(device const float* x [[buffer(0)]], device const float* v [[buffer(1)]],
                          device float* out [[buffer(2)]], constant uint4& p [[buffer(3)]],
                          uint i [[thread_position_in_grid]]) {
    if (i >= p.x) return;
    out[i] = x[i] + v[(i / p.y) % p.z];
}

kernel void bcast_mul_f32(device const float* x [[buffer(0)]], device const float* v [[buffer(1)]],
                          device float* out [[buffer(2)]], constant uint4& p [[buffer(3)]],
                          uint i [[thread_position_in_grid]]) {
    if (i >= p.x) return;
    out[i] = x[i] * v[(i / p.y) % p.z];
}

kernel void snake_f32(device const float* x [[buffer(0)]], device const float* alpha [[buffer(1)]],
                      device float* out [[buffer(2)]], constant uint4& p [[buffer(3)]],
                      uint i [[thread_position_in_grid]]) {
    if (i >= p.x) return;
    float a = alpha[(i / p.y) % p.z];
    float s = sin(a * x[i]);
    out[i] = x[i] + (1.0f / (a + 1e-9f)) * s * s;
}

kernel void bcast_fma_f32(device const float* a [[buffer(0)]], device const float* v [[buffer(1)]],
                          device const float* b [[buffer(2)]], device float* out [[buffer(3)]],
                          constant uint4& p [[buffer(4)]], uint i [[thread_position_in_grid]]) {
    if (i >= p.x) return;
    out[i] = a[i] + v[(i / p.y) % p.z] * b[i];
}

// in: (A, B1, B2, C) -> out: (A, B2, B1, C)。p = (n, B1, B2, C)
kernel void swap12_f32(device const float* x [[buffer(0)]], device float* out [[buffer(1)]],
                       constant uint4& p [[buffer(2)]], uint i [[thread_position_in_grid]]) {
    if (i >= p.x) return;
    uint c = i % p.w;
    uint t = i / p.w;
    uint i1 = t % p.y;
    t /= p.y;
    uint i2 = t % p.z;
    uint a = t / p.z;
    out[i] = x[((a * p.y + i1) * p.z + i2) * p.w + c];
}

// 1 行を 1 simdgroup (32 スレッド) で処理する。p = (rows, D, G), eps
kernel void rms_norm_grouped_f32(device const float* x [[buffer(0)]], device const float* w [[buffer(1)]],
                                 device float* out [[buffer(2)]], constant uint4& p [[buffer(3)]],
                                 constant float& eps [[buffer(4)]],
                                 uint row [[threadgroup_position_in_grid]],
                                 uint lane [[thread_index_in_threadgroup]]) {
    if (row >= p.x) return;
    uint d = p.y;
    device const float* xr = x + row * d;
    float acc = 0.0f;
    for (uint j = lane; j < d; j += 32) acc += xr[j] * xr[j];
    acc = simd_sum(acc);
    float r = rsqrt(acc / float(d) + eps);
    device const float* wr = w + (row % p.z) * d;
    device float* o = out + row * d;
    for (uint j = lane; j < d; j += 32) o[j] = xr[j] * r * wr[j];
}
"#;

    type Pipelines = HashMap<&'static str, ComputePipeline>;

    fn pipeline(device: &candle_core::MetalDevice, name: &'static str) -> Result<ComputePipeline> {
        static CACHE: OnceLock<Mutex<HashMap<u64, Arc<Pipelines>>>> = OnceLock::new();
        let md = device.metal_device();
        let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
        let pipes = match cache.get(&md.registry_id()) {
            Some(p) => p.clone(),
            None => {
                let lib = md
                    .new_library_with_source(SOURCE, None)
                    .map_err(candle_core::Error::wrap)?;
                let mut pipes = Pipelines::new();
                for n in [
                    "bcast_add_f32",
                    "bcast_mul_f32",
                    "snake_f32",
                    "bcast_fma_f32",
                    "swap12_f32",
                    "rms_norm_grouped_f32",
                ] {
                    let f = lib
                        .get_function(n, None)
                        .map_err(candle_core::Error::wrap)?;
                    pipes.insert(
                        n,
                        md.new_compute_pipeline_state_with_function(&f)
                            .map_err(candle_core::Error::wrap)?,
                    );
                }
                let pipes = Arc::new(pipes);
                cache.insert(md.registry_id(), pipes.clone());
                pipes
            }
        };
        Ok(pipes[name].clone())
    }

    fn check(s: &MetalStorage, l: &Layout, what: &str) -> Result<usize> {
        if s.dtype() != candle_core::DType::F32 || !l.is_contiguous() {
            bail!("{what}: expected a contiguous f32 tensor");
        }
        Ok(l.start_offset() * 4)
    }

    fn output(
        device: &candle_core::MetalDevice,
        n: usize,
        label: &str,
    ) -> Result<Arc<candle_metal_kernels::metal::Buffer>> {
        device
            .new_buffer_builder()
            .with_size_for(n, candle_core::DType::F32)
            .with_label(label)
            .build()
    }

    fn grid(n: usize) -> (MTLSize, MTLSize) {
        (
            MTLSize {
                width: n,
                height: 1,
                depth: 1,
            },
            MTLSize {
                width: n.min(256),
                height: 1,
                depth: 1,
            },
        )
    }

    fn params(n: usize, a: usize, b: usize, c: usize) -> Result<[u32; 4]> {
        let cast = |v: usize| {
            u32::try_from(v).map_err(|_| candle_core::Error::Msg("tensor too large".into()))
        };
        Ok([cast(n)?, cast(a)?, cast(b)?, cast(c)?])
    }

    struct Binary {
        div: usize,
        kernel: &'static str,
    }

    impl CustomOp2 for Binary {
        fn name(&self) -> &'static str {
            self.kernel
        }

        fn cpu_fwd(
            &self,
            _: &CpuStorage,
            _: &Layout,
            _: &CpuStorage,
            _: &Layout,
        ) -> Result<(CpuStorage, Shape)> {
            bail!("{} is metal only", self.kernel)
        }

        fn metal_fwd(
            &self,
            x: &MetalStorage,
            xl: &Layout,
            v: &MetalStorage,
            vl: &Layout,
        ) -> Result<(MetalStorage, Shape)> {
            let (xo, vo) = (check(x, xl, self.kernel)?, check(v, vl, self.kernel)?);
            let device = x.device();
            let n = xl.shape().elem_count();
            let out = output(device, n, self.kernel)?;
            let p = params(n, self.div, vl.shape().elem_count(), 0)?;
            let encoder = device.command_encoder()?;
            encoder.set_compute_pipeline_state(&pipeline(device, self.kernel)?);
            let enc = encoder.as_ref();
            enc.set_input_buffer(0, Some(x.buffer()), xo);
            enc.set_input_buffer(1, Some(v.buffer()), vo);
            enc.set_output_buffer(2, Some(&out), 0);
            enc.set_bytes(3, &p);
            let (g, t) = grid(n);
            enc.dispatch_threads(g, t);
            Ok((
                MetalStorage::new(out, device.clone(), n, candle_core::DType::F32),
                xl.shape().clone(),
            ))
        }
    }

    pub fn binary(x: &Tensor, v: &Tensor, div: usize, kernel: &'static str) -> Result<Tensor> {
        x.contiguous()?
            .apply_op2_no_bwd(&v.contiguous()?, &Binary { div, kernel })
    }

    struct Fma {
        div: usize,
    }

    impl CustomOp3 for Fma {
        fn name(&self) -> &'static str {
            "bcast_fma"
        }

        fn cpu_fwd(
            &self,
            _: &CpuStorage,
            _: &Layout,
            _: &CpuStorage,
            _: &Layout,
            _: &CpuStorage,
            _: &Layout,
        ) -> Result<(CpuStorage, Shape)> {
            bail!("bcast_fma is metal only")
        }

        fn metal_fwd(
            &self,
            a: &MetalStorage,
            al: &Layout,
            v: &MetalStorage,
            vl: &Layout,
            b: &MetalStorage,
            bl: &Layout,
        ) -> Result<(MetalStorage, Shape)> {
            let (ao, vo, bo) = (
                check(a, al, "fma")?,
                check(v, vl, "fma")?,
                check(b, bl, "fma")?,
            );
            let device = a.device();
            let n = al.shape().elem_count();
            let out = output(device, n, "bcast_fma")?;
            let p = params(n, self.div, vl.shape().elem_count(), 0)?;
            let encoder = device.command_encoder()?;
            encoder.set_compute_pipeline_state(&pipeline(device, "bcast_fma_f32")?);
            let enc = encoder.as_ref();
            enc.set_input_buffer(0, Some(a.buffer()), ao);
            enc.set_input_buffer(1, Some(v.buffer()), vo);
            enc.set_input_buffer(2, Some(b.buffer()), bo);
            enc.set_output_buffer(3, Some(&out), 0);
            enc.set_bytes(4, &p);
            let (g, t) = grid(n);
            enc.dispatch_threads(g, t);
            Ok((
                MetalStorage::new(out, device.clone(), n, candle_core::DType::F32),
                al.shape().clone(),
            ))
        }
    }

    pub fn fma(a: &Tensor, v: &Tensor, b: &Tensor, div: usize) -> Result<Tensor> {
        a.contiguous()?
            .apply_op3_no_bwd(&v.contiguous()?, &b.contiguous()?, &Fma { div })
    }

    struct Swap12;

    impl CustomOp1 for Swap12 {
        fn name(&self) -> &'static str {
            "swap12"
        }

        fn cpu_fwd(&self, _: &CpuStorage, _: &Layout) -> Result<(CpuStorage, Shape)> {
            bail!("swap12 is metal only")
        }

        fn metal_fwd(&self, x: &MetalStorage, xl: &Layout) -> Result<(MetalStorage, Shape)> {
            let xo = check(x, xl, "swap12")?;
            let dims = xl.dims();
            let (a, b1, b2, c) = (dims[0], dims[1], dims[2], dims[3]);
            let device = x.device();
            let n = a * b1 * b2 * c;
            let out = output(device, n, "swap12")?;
            let p = params(n, b1, b2, c)?;
            let encoder = device.command_encoder()?;
            encoder.set_compute_pipeline_state(&pipeline(device, "swap12_f32")?);
            let enc = encoder.as_ref();
            enc.set_input_buffer(0, Some(x.buffer()), xo);
            enc.set_output_buffer(1, Some(&out), 0);
            enc.set_bytes(2, &p);
            let (g, t) = grid(n);
            enc.dispatch_threads(g, t);
            Ok((
                MetalStorage::new(out, device.clone(), n, candle_core::DType::F32),
                Shape::from((a, b2, b1, c)),
            ))
        }
    }

    pub fn swap12(x: &Tensor) -> Result<Tensor> {
        x.contiguous()?.apply_op1_no_bwd(&Swap12)
    }

    struct RmsNormGrouped {
        eps: f32,
    }

    impl CustomOp2 for RmsNormGrouped {
        fn name(&self) -> &'static str {
            "rms_norm_grouped"
        }

        fn cpu_fwd(
            &self,
            _: &CpuStorage,
            _: &Layout,
            _: &CpuStorage,
            _: &Layout,
        ) -> Result<(CpuStorage, Shape)> {
            bail!("rms_norm_grouped is metal only")
        }

        fn metal_fwd(
            &self,
            x: &MetalStorage,
            xl: &Layout,
            w: &MetalStorage,
            wl: &Layout,
        ) -> Result<(MetalStorage, Shape)> {
            let (xo, wo) = (check(x, xl, "rms_norm")?, check(w, wl, "rms_norm")?);
            let d = *xl.dims().last().unwrap();
            let n = xl.shape().elem_count();
            let rows = n / d;
            let groups = wl.shape().elem_count() / d;
            let device = x.device();
            let out = output(device, n, "rms_norm_grouped")?;
            let p = params(rows, d, groups, 0)?;
            let encoder = device.command_encoder()?;
            encoder.set_compute_pipeline_state(&pipeline(device, "rms_norm_grouped_f32")?);
            let enc = encoder.as_ref();
            enc.set_input_buffer(0, Some(x.buffer()), xo);
            enc.set_input_buffer(1, Some(w.buffer()), wo);
            enc.set_output_buffer(2, Some(&out), 0);
            enc.set_bytes(3, &p);
            enc.set_bytes(4, &self.eps);
            enc.dispatch_thread_groups(
                MTLSize {
                    width: rows,
                    height: 1,
                    depth: 1,
                },
                MTLSize {
                    width: 32,
                    height: 1,
                    depth: 1,
                },
            );
            Ok((
                MetalStorage::new(out, device.clone(), n, candle_core::DType::F32),
                xl.shape().clone(),
            ))
        }
    }

    pub fn rms_norm_grouped(x: &Tensor, w: &Tensor, eps: f32) -> Result<Tensor> {
        x.contiguous()?
            .apply_op2_no_bwd(&w.contiguous()?, &RmsNormGrouped { eps })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;

    fn max_diff(a: &Tensor, b: &Tensor) -> f32 {
        (a.to_device(&Device::Cpu).unwrap() - b.to_device(&Device::Cpu).unwrap())
            .unwrap()
            .abs()
            .unwrap()
            .flatten_all()
            .unwrap()
            .max(0)
            .unwrap()
            .to_scalar::<f32>()
            .unwrap()
    }

    /// 選択デバイスの実装と CPU の実装が一致すること (Metal 有効時にカーネルを検証する)。
    #[test]
    fn ops_match_cpu() -> Result<()> {
        let dev = crate::select_device().unwrap();
        let cpu = Device::Cpu;
        let x = Tensor::randn(0f32, 1., (2, 3, 5, 8), &cpu)?;
        let y = Tensor::randn(0f32, 1., (2, 3, 5, 8), &cpu)?;
        let row = Tensor::randn(0f32, 1., 8, &cpu)?;
        let chan = Tensor::randn(0f32, 1., 3, &cpu)?.abs()?;
        let w = Tensor::randn(0f32, 1., (5, 8), &cpu)?;
        let (xd, yd, rowd, wd) = (
            x.to_device(&dev)?,
            y.to_device(&dev)?,
            row.to_device(&dev)?,
            w.to_device(&dev)?,
        );
        let x3 = x.reshape((2, 3, 40))?;
        let x3d = x3.to_device(&dev)?;
        let chand = chan.to_device(&dev)?;

        assert!(
            max_diff(
                &bcast_add(&xd, &rowd, Axis::Last)?,
                &bcast_add(&x, &row, Axis::Last)?
            ) < 1e-6
        );
        assert!(
            max_diff(
                &bcast_mul(&x3d, &chand, Axis::Channel)?,
                &bcast_mul(&x3, &chan, Axis::Channel)?
            ) < 1e-6
        );
        assert!(
            max_diff(
                &bcast_fma(&xd, &rowd, &yd, Axis::Last)?,
                &bcast_fma(&x, &row, &y, Axis::Last)?
            ) < 1e-5
        );
        assert!(max_diff(&snake(&x3d, &chand)?, &snake(&x3, &chan)?) < 1e-4);
        assert!(
            max_diff(
                &rms_norm_grouped(&xd, &wd, 1e-5)?,
                &rms_norm_grouped(&x, &w, 1e-5)?
            ) < 1e-5
        );
        assert!(max_diff(&swap12(&xd)?, &swap12(&x)?) == 0.0);
        Ok(())
    }
}

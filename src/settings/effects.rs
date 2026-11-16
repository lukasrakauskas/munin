//! A small, finite rune-fx celebration around the successful pairing banner.
use crate::state::State;
use rune::compose::{Compositor, DeviceKey, Frame};
use rune_fx::{
    Fx, FxFrame, FxLayer, FxOptions, FxRenderer, GlowParams, GlowStyle, Shape, SparkleParams, rgb,
};

pub const LIFE: f32 = 1.8;
pub fn layers(rect: [f32; 4], elapsed: f32, reduced: bool, seed: u32) -> Vec<FxLayer> {
    if reduced
        || !elapsed.is_finite()
        || !(0.0..LIFE).contains(&elapsed)
        || rect[2] <= 0.0
        || rect[3] <= 0.0
    {
        return Vec::new();
    }
    let shape = Shape::rounded(rect[0], rect[1], rect[2], rect[3], 10.0);
    let fade = (1.0 - elapsed / LIFE).powi(2);
    vec![
        Fx::glow(GlowParams {
            style: GlowStyle::Neon,
            color: rgb(0x8c72ff),
            core: rgb(0xebfff7),
            radius: 12.0,
            intensity: 1.4 * fade,
            occlusion: 0.25,
            ..Default::default()
        })
        .on(shape)
        .over()
        .seed(seed)
        .at(elapsed),
        Fx::sparkles(SparkleParams {
            color: rgb(0x8ff2cf),
            count: 20,
            life: 0.6,
            size: 5.5,
            margin: 18.0,
            intensity: fade,
            burst: Some(0.0),
        })
        .on(shape)
        .over()
        .seed(seed)
        .at(elapsed),
    ]
}
#[derive(Default)]
pub struct PairingFx {
    resources: Option<(DeviceKey, wgpu::TextureFormat, FxRenderer)>,
    sequence: Option<u64>,
    started: f64,
    running: bool,
}
impl Compositor<State> for PairingFx {
    fn animating(&self) -> bool {
        self.running
    }
    fn render(&mut self, f: &mut Frame<'_, State>) {
        f.draw_ui();
        self.running = false;
        let Some(success) = &f.rt.state.sync.success else {
            return;
        };
        let Some((rect, opacity)) = f.rt.presented("pairing-success").filter(|(_, a)| *a > 0.0)
        else {
            return;
        };
        if self.sequence != Some(success.sequence) {
            self.sequence = Some(success.sequence);
            self.started = f.rt.t;
        }
        let elapsed = (f.rt.t - self.started) as f32;
        let layers: Vec<_> = layers(rect, elapsed, f.rt.reduced_motion, success.sequence as u32)
            .into_iter()
            .map(|l| l.opacity(opacity))
            .collect();
        if layers.is_empty() {
            return;
        }
        self.running = true;
        let key = f.device_key();
        if self
            .resources
            .as_ref()
            .is_none_or(|(old, format, _)| *old != key || *format != f.format)
        {
            self.resources = Some((
                key,
                f.format,
                FxRenderer::new(f.device, f.format, FxOptions::default()),
            ));
        }
        let srgb = f.srgb_blend();
        let fx = &mut self.resources.as_mut().unwrap().2;
        fx.set_srgb_blend(srgb);
        fx.prepare(
            f.device,
            f.queue,
            &FxFrame {
                viewport: [f.size.0, f.size.1],
                scale: f.rt.env.scale,
                layers: &layers,
                backdrop: None,
                sources: &[],
            },
        );
        let mut pass = f.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pairing celebration"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: f.target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        fx.draw_all(&mut pass, &layers, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an actual GPU/software graphics adapter; run explicitly"]
    fn gpu_pairing_smoke_draws_then_goes_idle() {
        use rune::render::{Gpu, Offscreen, Renderer};
        use rune::{Env, Runtime};
        let gpu = Gpu::headless();
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let target = Offscreen::with_format(&gpu.device, 1200, 800, format);
        let mut ui = Renderer::new(&gpu.device, format);
        let mut state = State::restore(None, None);
        state.settings = true;
        state.sync.success = Some(crate::sync::Success {
            name: "Studio laptop".into(),
            sequence: 1,
        });
        let mut rt = Runtime::new(
            state,
            crate::view,
            (1200.0, 800.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        rt.frame(0.3);
        let mut fx = PairingFx::default();
        let mut draw = |fx: &mut PairingFx, rt: &mut Runtime<State>| {
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            let mut frame = Frame {
                device: &gpu.device,
                queue: &gpu.queue,
                encoder: &mut encoder,
                target: &target.view,
                format,
                size: (1200, 800),
                dt: 0.0,
                update_ms: 0.0,
                rt,
                ui: &mut ui,
            };
            fx.render(&mut frame);
            target.copy(&mut encoder);
            gpu.queue.submit([encoder.finish()]);
            target.read(&gpu.device)
        };
        rt.reduced_motion = true;
        let baseline = draw(&mut fx, &mut rt);
        assert!(!fx.animating());
        rt.reduced_motion = false;
        rt.t += 0.3;
        let lit = draw(&mut fx, &mut rt);
        assert!(fx.animating());
        assert!(
            baseline != lit,
            "rune-fx must actually draw, not just update state"
        );
        rune::render::write_png(
            &std::env::temp_dir().join("munin-pairing-success.png"),
            1200,
            800,
            &lit,
        )
        .unwrap();
        rt.t += f64::from(LIFE) + 0.1;
        let ended = draw(&mut fx, &mut rt);
        assert!(
            !fx.animating(),
            "finished one-shot must not keep requesting frames"
        );
        assert!(
            baseline == ended,
            "finished effects must leave the ordinary UI unchanged"
        );
    }

    #[test]
    fn celebration_is_bounded_and_has_no_reduced_motion_or_idle_frames() {
        let rect = [10.0, 20.0, 500.0, 80.0];
        assert!(!layers(rect, 0.3, false, 7).is_empty());
        assert!(
            layers(rect, 0.3, true, 7).is_empty(),
            "reduced motion keeps the text, never travelling particles"
        );
        assert!(layers(rect, LIFE, false, 7).is_empty());
        assert!(layers(rect, -0.1, false, 7).is_empty());
        assert!(layers([0.0, 0.0, 0.0, 0.0], 0.3, false, 7).is_empty());
    }
}

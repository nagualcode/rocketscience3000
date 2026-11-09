//! Offline Roger.
//!
//! ```text
//! roger-render in.wav out.wav [--vowel 3] [--family 1] [--bandwidth 30] [--auto] [--bpm 120] …
//! roger-render --scenario scenario.json out.f32     # for the differential test
//! ```

use roger::engine::{receive_midi, Controls, HostTime, Layout, Roger};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--scenario") => scenario(&args[1], &args[2]),
        Some(_) if args.len() >= 2 => render(&args),
        _ => {
            eprintln!(
                "usage: roger-render in.wav out.wav [--vowel 0-9] [--family 0-2] [--bandwidth 1-100] \
                 [--portamento 1-100] [--auto] [--speed 1-12] [--skip 0-100] [--bpm N]\n       \
                 roger-render --scenario scenario.json out.f32"
            );
            std::process::exit(2);
        }
    }
}

fn apply(c: &mut Controls, m: &serde_json::Map<String, serde_json::Value>) {
    for (k, v) in m {
        let f = v.as_f64().expect("number") as i32;
        match k.as_str() {
            "bandwidth" => c.bandwidth = f,
            "portamento" => c.portamento = f,
            "auto_speed" => c.auto_speed = f,
            "width_mode" => c.width_mode = f,
            "port_mode" => c.port_mode = f,
            "auto" => c.auto = f != 0,
            "vowel" => c.vowel = f,
            "family" => c.family = f,
            "skip" => c.skip = f,
            _ => panic!("unknown control {k}"),
        }
    }
}

fn scenario(path: &str, out_path: &str) {
    let sc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("scenario")).expect("json");
    let sr = sc["sample_rate"].as_f64().unwrap();
    let n = sc["block"].as_u64().unwrap() as usize;
    let blocks = sc["blocks"].as_u64().unwrap();
    let layout = match sc["layout"].as_str().unwrap_or("stereo") {
        "mono" => Layout::Mono,
        "mono_to_stereo" => Layout::MonoToStereo,
        _ => Layout::Stereo,
    };
    let mut c = Controls::default();
    if let Some(p) = sc["params"].as_object() {
        apply(&mut c, p);
    }
    let raw = std::fs::read(sc["input"].as_str().unwrap()).expect("input");
    let sig: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let left: Vec<f32> = sig.iter().step_by(2).copied().collect();
    let right: Vec<f32> = sig.iter().skip(1).step_by(2).copied().collect();
    let mut rg = Roger::new(sr, layout, c.bandwidth);
    let changes = sc["changes"].as_object().cloned().unwrap_or_default();
    let midi = sc["midi"].as_object().cloned().unwrap_or_default();
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).expect("output"));
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let _ftz = mission::denormal::DenormalFlush::on();
    for b in 0..blocks {
        if let Some(m) = changes.get(&b.to_string()).and_then(|v| v.as_object()) {
            apply(&mut c, m);
        }
        if let Some(evs) = midi.get(&b.to_string()).and_then(|v| v.as_array()) {
            for e in evs {
                let e: Vec<u8> = e.as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u8).collect();
                receive_midi(&mut c, e[0], e[1], e[2]);
            }
        }
        let host = match sc["transport"].as_object() {
            Some(t) => {
                let bpm = t["bpm"].as_f64().unwrap();
                let ppq = t.get("start_ppq").and_then(|v| v.as_f64()).unwrap_or(0.0) + b as f64 * n as f64 / (sr * 60.0 / bpm);
                HostTime::new(sr, bpm, ppq, b == 0)
            }
            // No host tempo: the context's constructor ran SetTempo(120) before
            // it knew the sample rate, so samples per quarter is 0 — and Auto
            // rolls a new vowel on every sample.
            None => HostTime { samples_per_quarter: 0.0, samples_to_next_beat: 0, changed: false },
        };
        let o = (b as usize * n) % (left.len() - n);
        l.copy_from_slice(&left[o..o + n]);
        r.copy_from_slice(&right[o..o + n]);
        rg.process(&mut c, &host, &mut l, &mut r);
        for k in 0..n {
            out.write_all(&l[k].to_le_bytes()).unwrap();
            if layout != Layout::Mono {
                out.write_all(&r[k].to_le_bytes()).unwrap();
            }
        }
    }
    println!("vowel={} fam={} bw={:.4} beat={:.3} F=({:.3},{:.3},{:.3})", c.vowel, c.family, rg.bandwidth, rg.beat, rg.formants[0], rg.formants[2], rg.formants[4]);
}

fn render(args: &[String]) {
    let mut c = Controls { auto: false, ..Controls::default() };
    let mut bpm = 120.0;
    let mut i = 2;
    while i < args.len() {
        let v = || args.get(i + 1).and_then(|s| s.parse::<f64>().ok()).expect("value");
        match args[i].as_str() {
            "--vowel" => c.vowel = v() as i32,
            "--family" => c.family = v() as i32,
            "--bandwidth" => c.bandwidth = v() as i32,
            "--portamento" => c.portamento = v() as i32,
            "--speed" => c.auto_speed = v() as i32,
            "--skip" => c.skip = v() as i32,
            "--bpm" => bpm = v(),
            "--auto" => {
                c.auto = true;
                i += 1;
                continue;
            }
            a => panic!("unknown option {a}"),
        }
        i += 2;
    }
    let mut rd = hound::WavReader::open(&args[0]).expect("input wav");
    let spec = rd.spec();
    let ch = spec.channels as usize;
    let data: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => rd.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            rd.samples::<i32>().map(|s| s.unwrap() as f32 * scale).collect()
        }
    };
    let sr = spec.sample_rate as f64;
    let frames = data.len() / ch;
    let layout = if ch == 1 { Layout::MonoToStereo } else { Layout::Stereo };
    let mut rg = Roger::new(sr, layout, c.bandwidth);
    let mut w = hound::WavWriter::create(&args[1], hound::WavSpec { channels: 2, sample_rate: spec.sample_rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float }).expect("output wav");
    let _ftz = mission::denormal::DenormalFlush::on();
    let n = 512;
    let mut pos = 0;
    while pos < frames {
        let m = n.min(frames - pos);
        let mut l: Vec<f32> = (0..m).map(|k| data[(pos + k) * ch]).collect();
        let mut r: Vec<f32> = (0..m).map(|k| data[(pos + k) * ch + (ch - 1)]).collect();
        let host = HostTime::new(sr, bpm, pos as f64 / (sr * 60.0 / bpm), pos == 0);
        rg.process(&mut c, &host, &mut l, &mut r);
        for k in 0..m {
            w.write_sample(l[k]).unwrap();
            w.write_sample(r[k]).unwrap();
        }
        pos += m;
    }
    w.finalize().unwrap();
}

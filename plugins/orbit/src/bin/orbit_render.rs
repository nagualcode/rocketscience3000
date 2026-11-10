//! Offline Orbit.
//!
//! ```text
//! orbit-render in.wav out.wav [--mode 3] [--speed 3] [--room 25] [--order 2] [--damp 0] [--buffer 512]
//! orbit-render --scenario scenario.json out.f32     # for the differential test
//! ```

use orbit::engine::{receive_midi, Controls, Layout, Orbit};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--scenario") => scenario(&args[1], &args[2]),
        Some(_) if args.len() >= 2 => render(&args),
        _ => {
            eprintln!(
                "usage: orbit-render in.wav out.wav [--mode 1-5] [--speed 0-99] [--room 8-80] [--order 0-5] \
                 [--damp 0-100] [--span 0.2-6] [--buffer N]\n       orbit-render --scenario scenario.json out.f32"
            );
            std::process::exit(2);
        }
    }
}

fn apply(c: &mut Controls, src: &mut (f32, f32), m: &serde_json::Map<String, serde_json::Value>) {
    for (k, v) in m {
        let f = v.as_f64().expect("number");
        match k.as_str() {
            "source_x" => src.0 = f as f32,
            "source_y" => src.1 = f as f32,
            "room" => c.room = f as f32,
            "order" => c.order = f as i32,
            "inner_y" => c.inner_y = f as f32,
            "inner_x" => c.inner_x = f as f32,
            "damp" => c.damp = f as i32,
            "speed" => c.speed = f as f32,
            "mode" => c.mode = f as i32,
            "rad_x" => c.rad_x = f as f32,
            "rad_y" => c.rad_y = f as f32,
            "pos_x" => c.pos_x = f as f32,
            "pos_y" => c.pos_y = f as f32,
            "span" => c.span = f as f32,
            "x" => c.x = f as f32,
            "y" => c.y = f as f32,
            _ => panic!("unknown control {k}"),
        }
    }
}

fn scenario(path: &str, out_path: &str) {
    let sc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("scenario")).expect("json");
    let sr = sc["sample_rate"].as_f64().unwrap();
    let n = sc["block"].as_u64().unwrap() as usize;
    let buffer = sc["buffer"].as_u64().map(|b| b as usize).unwrap_or(n);
    let blocks = sc["blocks"].as_u64().unwrap();
    let layout = match sc["layout"].as_str().unwrap_or("mono_to_stereo") {
        "stereo" => Layout::Stereo,
        _ => Layout::MonoToStereo,
    };
    let mut c = Controls::default();
    let mut src = (0.0f32, 0.0f32);
    if let Some(p) = sc["params"].as_object() {
        apply(&mut c, &mut src, p);
    }
    let raw = std::fs::read(sc["input"].as_str().unwrap()).expect("input");
    let sig: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let left: Vec<f32> = sig.iter().step_by(2).copied().collect();
    let right: Vec<f32> = sig.iter().skip(1).step_by(2).copied().collect();
    let mut ob = Orbit::new(sr, layout, buffer, &c, src);
    let changes = sc["changes"].as_object().cloned().unwrap_or_default();
    let midi = sc["midi"].as_object().cloned().unwrap_or_default();
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).expect("output"));
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let _ftz = mission::denormal::DenormalFlush::on();
    for b in 0..blocks {
        if let Some(m) = changes.get(&b.to_string()).and_then(|v| v.as_object()) {
            let mut s = ob.source;
            apply(&mut c, &mut s, m);
            ob.source = s;
        }
        if let Some(evs) = midi.get(&b.to_string()).and_then(|v| v.as_array()) {
            for e in evs {
                let e: Vec<u8> = e.as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u8).collect();
                receive_midi(&mut c, e[0], e[1], e[2]);
            }
        }
        let o = (b as usize * n) % (left.len() - n);
        l.copy_from_slice(&left[o..o + n]);
        r.copy_from_slice(&right[o..o + n]);
        ob.process(&c, &mut l, &mut r);
        for k in 0..n {
            out.write_all(&l[k].to_le_bytes()).unwrap();
            out.write_all(&r[k].to_le_bytes()).unwrap();
        }
    }
    println!("src=({:.6},{:.6})", ob.source.0, ob.source.1);
}

fn render(args: &[String]) {
    let mut c = Controls::default();
    let mut buffer = 512usize;
    let mut i = 2;
    while i < args.len() {
        let v = args.get(i + 1).and_then(|s| s.parse::<f64>().ok()).expect("value");
        match args[i].as_str() {
            "--mode" => c.mode = v as i32,
            "--speed" => c.speed = v as f32,
            "--room" => c.room = v as f32,
            "--order" => c.order = v as i32,
            "--damp" => c.damp = v as i32,
            "--span" => c.span = v as f32,
            "--buffer" => buffer = v as usize,
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
    let frames = data.len() / ch;
    let layout = if ch == 1 { Layout::MonoToStereo } else { Layout::Stereo };
    let mut ob = Orbit::new(spec.sample_rate as f64, layout, buffer, &c, (0.0, 0.0));
    let mut w = hound::WavWriter::create(&args[1], hound::WavSpec { channels: 2, sample_rate: spec.sample_rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float }).expect("output wav");
    let _ftz = mission::denormal::DenormalFlush::on();
    // A second of tail for the reflections.
    let total = frames + spec.sample_rate as usize;
    let mut pos = 0;
    while pos < total {
        let mut l: Vec<f32> = (0..buffer).map(|k| data.get((pos + k) * ch).copied().unwrap_or(0.0)).collect();
        let mut r: Vec<f32> = (0..buffer).map(|k| data.get((pos + k) * ch + (ch - 1)).copied().unwrap_or(0.0)).collect();
        ob.process(&c, &mut l, &mut r);
        for k in 0..buffer {
            w.write_sample(l[k]).unwrap();
            w.write_sample(r[k]).unwrap();
        }
        pos += buffer;
    }
    w.finalize().unwrap();
}

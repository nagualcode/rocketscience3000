//! Pieces of Audio Ease's "Mission" plug-in framework, shared by the Rocket
//! Science plug-ins (and, in nautilus3000, by the Nautilus Bundle). The same
//! AGC / look-ahead peak limiter sits behind every one of them, under a
//! different obfuscated class name per bundle (`wkbtgem…` in Rocket Science,
//! `yruwg…` in Nautilus).

pub mod agc;
pub mod breakpoint;
pub mod denormal;
pub mod fft;
pub mod ring;

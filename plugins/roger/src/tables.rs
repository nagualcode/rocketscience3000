//! Roger's vowel table (`_form` @ 0x3cba0): 3 family members × 10 vowels,
//! each `[F1, A1, F2, A2, F3, A3]` — formant frequencies in Hz and linear
//! amplitudes. These are Peterson & Barney's 1952 averages for men (Roger),
//! women (Patty) and children (Cindy), the amplitudes converted from dB.
//! Bit patterns copied from the binary.

/// Vowel names as `RogerVowelParm::GetValueString` prints them, with the
/// manual's key words.
pub const VOWELS: [(&str, &str); 10] = [
    ("OO", "bOOt"), ("AW", "lAw"), ("UH", "dUH"), ("AH", "utAH"), ("U", "bUt"),
    ("IR", "bIRd"), ("A", "bAt"), ("E", "bEt"), ("I", "bIt"), ("EE", "bEEt"),
];

/// `RogerFamilyMemberParm::GetValueString`: 0 Cindy, 1 Roger, 2 Patty.
pub const FAMILY: [&str; 3] = ["Cindy", "Roger", "Patty"];

pub const FORM: [[f64; 6]; 30] = [
    // Cindy OO: 430 / 1170 / 3260 Hz
    [f64::from_bits(0x407ae00000000000), f64::from_bits(0x3fe6a77de88e1e66), f64::from_bits(0x4092480000000000), f64::from_bits(0x3fbcb942ac4cfa85), f64::from_bits(0x40a9780000000000), f64::from_bits(0x3f7cff597b973127)],
    // Cindy AW: 680 / 1060 / 3180 Hz
    [f64::from_bits(0x4085400000000000), f64::from_bits(0x3ff0000000000000), f64::from_bits(0x4090900000000000), f64::from_bits(0x3fdc9676c4ad6e8e), f64::from_bits(0x40a8d80000000000), f64::from_bits(0x3f946e75dd0485ff)],
    // Cindy UH: 560 / 1410 / 3310 Hz
    [f64::from_bits(0x4081800000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x4096080000000000), f64::from_bits(0x3fd01379847b4f57), f64::from_bits(0x40a9dc0000000000), f64::from_bits(0x3f946e75dd0485ff)],
    // Cindy AH: 1030 / 1370 / 3170 Hz
    [f64::from_bits(0x4090180000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x4095680000000000), f64::from_bits(0x3fe1feb33eb140aa), f64::from_bits(0x40a8c40000000000), f64::from_bits(0x3fa46211ff173309)],
    // Cindy U: 850 / 1590 / 3370 Hz
    [f64::from_bits(0x408a900000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x4098d80000000000), f64::from_bits(0x3fd43d13668f2b2c), f64::from_bits(0x40aa540000000000), f64::from_bits(0x3fa6dec56a245871)],
    // Cindy IR: 560 / 1820 / 2160 Hz
    [f64::from_bits(0x4081800000000000), f64::from_bits(0x3fe1feb33eb140aa), f64::from_bits(0x409c700000000000), f64::from_bits(0x3fc6c310e14eb9f7), f64::from_bits(0x40a0e00000000000), f64::from_bits(0x3fb999999999999a)],
    // Cindy A: 1010 / 2320 / 3320 Hz
    [f64::from_bits(0x408f900000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x40a2200000000000), f64::from_bits(0x3fd01379847b4f57), f64::from_bits(0x40a9f00000000000), f64::from_bits(0x3fb455b5a10392a9)],
    // Cindy E: 690 / 2610 / 3570 Hz
    [f64::from_bits(0x4085900000000000), f64::from_bits(0x3fe96b2309447753), f64::from_bits(0x40a4640000000000), f64::from_bits(0x3fc2149a5657fb6a), f64::from_bits(0x40abe40000000000), f64::from_bits(0x3fb0270ac20c0fbf)],
    // Cindy I: 530 / 2730 / 3600 Hz
    [f64::from_bits(0x4080900000000000), f64::from_bits(0x3fe6a77de88e1e66), f64::from_bits(0x40a5540000000000), f64::from_bits(0x3fb21f97ed3e7eb8), f64::from_bits(0x40ac200000000000), f64::from_bits(0x3fa6dec56a245871)],
    // Cindy EE: 370 / 3220 / 3730 Hz
    [f64::from_bits(0x4077200000000000), f64::from_bits(0x3fe430cd728f13af), f64::from_bits(0x40a9280000000000), f64::from_bits(0x3fb0270ac20c0fbf), f64::from_bits(0x40ad240000000000), f64::from_bits(0x3fa46211ff173309)],
    // Roger OO: 300 / 870 / 2240 Hz
    [f64::from_bits(0x4072c00000000000), f64::from_bits(0x3fe6a77de88e1e66), f64::from_bits(0x408b300000000000), f64::from_bits(0x3fbcb942ac4cfa85), f64::from_bits(0x40a1800000000000), f64::from_bits(0x3f7cff597b973127)],
    // Roger AW: 570 / 840 / 2410 Hz
    [f64::from_bits(0x4081d00000000000), f64::from_bits(0x3ff0000000000000), f64::from_bits(0x408a400000000000), f64::from_bits(0x3fdc9676c4ad6e8e), f64::from_bits(0x40a2d40000000000), f64::from_bits(0x3f946e75dd0485ff)],
    // Roger UH: 440 / 1020 / 2240 Hz
    [f64::from_bits(0x407b800000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x408fe00000000000), f64::from_bits(0x3fd01379847b4f57), f64::from_bits(0x40a1800000000000), f64::from_bits(0x3f946e75dd0485ff)],
    // Roger AH: 730 / 1090 / 2440 Hz
    [f64::from_bits(0x4086d00000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x4091080000000000), f64::from_bits(0x3fe1feb33eb140aa), f64::from_bits(0x40a3100000000000), f64::from_bits(0x3fa46211ff173309)],
    // Roger U: 640 / 1190 / 2390 Hz
    [f64::from_bits(0x4084000000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x4092980000000000), f64::from_bits(0x3fd43d13668f2b2c), f64::from_bits(0x40a2ac0000000000), f64::from_bits(0x3fa6dec56a245871)],
    // Roger IR: 490 / 1350 / 1690 Hz
    [f64::from_bits(0x407ea00000000000), f64::from_bits(0x3fe1feb33eb140aa), f64::from_bits(0x4095180000000000), f64::from_bits(0x3fc6c310e14eb9f7), f64::from_bits(0x409a680000000000), f64::from_bits(0x3fb999999999999a)],
    // Roger A: 660 / 1720 / 2410 Hz
    [f64::from_bits(0x4084a00000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x409ae00000000000), f64::from_bits(0x3fd01379847b4f57), f64::from_bits(0x40a2d40000000000), f64::from_bits(0x3fb455b5a10392a9)],
    // Roger E: 530 / 1840 / 2480 Hz
    [f64::from_bits(0x4080900000000000), f64::from_bits(0x3fe96b2309447753), f64::from_bits(0x409cc00000000000), f64::from_bits(0x3fc2149a5657fb6a), f64::from_bits(0x40a3600000000000), f64::from_bits(0x3fb0270ac20c0fbf)],
    // Roger I: 390 / 1990 / 2550 Hz
    [f64::from_bits(0x4078600000000000), f64::from_bits(0x3fe6a77de88e1e66), f64::from_bits(0x409f180000000000), f64::from_bits(0x3fb21f97ed3e7eb8), f64::from_bits(0x40a3ec0000000000), f64::from_bits(0x3fa6dec56a245871)],
    // Roger EE: 270 / 2290 / 3010 Hz
    [f64::from_bits(0x4070e00000000000), f64::from_bits(0x3fe430cd728f13af), f64::from_bits(0x40a1e40000000000), f64::from_bits(0x3fb0270ac20c0fbf), f64::from_bits(0x40a7840000000000), f64::from_bits(0x3fa46211ff173309)],
    // Patty OO: 370 / 950 / 2670 Hz
    [f64::from_bits(0x4077200000000000), f64::from_bits(0x3fe6a77de88e1e66), f64::from_bits(0x408db00000000000), f64::from_bits(0x3fbcb942ac4cfa85), f64::from_bits(0x40a4dc0000000000), f64::from_bits(0x3f7cff597b973127)],
    // Patty AW: 590 / 920 / 2710 Hz
    [f64::from_bits(0x4082700000000000), f64::from_bits(0x3ff0000000000000), f64::from_bits(0x408cc00000000000), f64::from_bits(0x3fdc9676c4ad6e8e), f64::from_bits(0x40a52c0000000000), f64::from_bits(0x3f946e75dd0485ff)],
    // Patty UH: 470 / 1160 / 2680 Hz
    [f64::from_bits(0x407d600000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x4092200000000000), f64::from_bits(0x3fd01379847b4f57), f64::from_bits(0x40a4f00000000000), f64::from_bits(0x3f946e75dd0485ff)],
    // Patty AH: 850 / 1220 / 2810 Hz
    [f64::from_bits(0x408a900000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x4093100000000000), f64::from_bits(0x3fe1feb33eb140aa), f64::from_bits(0x40a5f40000000000), f64::from_bits(0x3fa46211ff173309)],
    // Patty U: 760 / 1400 / 2780 Hz
    [f64::from_bits(0x4087c00000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x4095e00000000000), f64::from_bits(0x3fd43d13668f2b2c), f64::from_bits(0x40a5b80000000000), f64::from_bits(0x3fa6dec56a245871)],
    // Patty IR: 500 / 1640 / 1960 Hz
    [f64::from_bits(0x407f400000000000), f64::from_bits(0x3fe1feb33eb140aa), f64::from_bits(0x4099a00000000000), f64::from_bits(0x3fc6c310e14eb9f7), f64::from_bits(0x409ea00000000000), f64::from_bits(0x3fb999999999999a)],
    // Patty A: 860 / 2050 / 2850 Hz
    [f64::from_bits(0x408ae00000000000), f64::from_bits(0x3fec8520b0fa8949), f64::from_bits(0x40a0040000000000), f64::from_bits(0x3fd01379847b4f57), f64::from_bits(0x40a6440000000000), f64::from_bits(0x3fb455b5a10392a9)],
    // Patty E: 610 / 2330 / 2990 Hz
    [f64::from_bits(0x4083100000000000), f64::from_bits(0x3fe96b2309447753), f64::from_bits(0x40a2340000000000), f64::from_bits(0x3fc2149a5657fb6a), f64::from_bits(0x40a75c0000000000), f64::from_bits(0x3fb0270ac20c0fbf)],
    // Patty I: 430 / 2480 / 3070 Hz
    [f64::from_bits(0x407ae00000000000), f64::from_bits(0x3fe6a77de88e1e66), f64::from_bits(0x40a3600000000000), f64::from_bits(0x3fb21f97ed3e7eb8), f64::from_bits(0x40a7fc0000000000), f64::from_bits(0x3fa6dec56a245871)],
    // Patty EE: 310 / 2790 / 3310 Hz
    [f64::from_bits(0x4073600000000000), f64::from_bits(0x3fe430cd728f13af), f64::from_bits(0x40a5cc0000000000), f64::from_bits(0x3fb0270ac20c0fbf), f64::from_bits(0x40a9dc0000000000), f64::from_bits(0x3fa46211ff173309)],
];

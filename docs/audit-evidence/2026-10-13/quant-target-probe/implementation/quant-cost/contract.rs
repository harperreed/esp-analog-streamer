// ABOUTME: Shares the throwaway quantizer kernel, bounded fixtures and output receipts.
// ABOUTME: Keeps host and target arithmetic identical without changing the production encoder.

pub const BLOCKS: usize = 16;
pub const CHUNK_SAMPLES: usize = 480;
pub const KINDS: [&str; 5] = [
    "silence",
    "quarter_dc",
    "triangle_fade",
    "noise24",
    "extrema",
];
pub const SEEDS: [u32; 2] = [0x1234_5678, 0x9abc_def1];
pub const HASH_INIT: u32 = 0x811c_9dc5;
pub const SOURCE_HASHES: [u32; 5] = [4021722565, 3567754693, 3803878314, 1329954454, 1467205061];

#[derive(Clone, Copy, Debug)]
pub enum Policy {
    Trunc,
    Round,
    Tpdf,
}

impl Policy {
    pub fn name(self) -> &'static str {
        match self {
            Self::Trunc => "trunc",
            Self::Round => "round",
            Self::Tpdf => "tpdf",
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Trunc => 0,
            Self::Round => 1,
            Self::Tpdf => 2,
        }
    }
}

pub const ORDERS: [[Policy; 3]; 6] = [
    [Policy::Trunc, Policy::Round, Policy::Tpdf],
    [Policy::Trunc, Policy::Tpdf, Policy::Round],
    [Policy::Round, Policy::Trunc, Policy::Tpdf],
    [Policy::Round, Policy::Tpdf, Policy::Trunc],
    [Policy::Tpdf, Policy::Trunc, Policy::Round],
    [Policy::Tpdf, Policy::Round, Policy::Trunc],
];

pub fn next_u32(state: &mut u32) -> u32 {
    assert_ne!(*state, 0, "xorshift seed must be nonzero");
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

fn saturate(q: i64) -> i32 {
    q.clamp(-32768, 32767) as i32
}

/// Discrete ties-up rounding: clamp only after wider addition and arithmetic shift.
pub fn dither_quantize(x: i32, a: u8, b: u8) -> i32 {
    saturate((i64::from(x) + i64::from(a) - i64::from(b) + 128) >> 8)
}

pub fn quantize(x: i32, policy: Policy, state: &mut u32) -> i32 {
    match policy {
        Policy::Trunc => (x >> 8).clamp(-32768, 32767),
        Policy::Round => saturate((i64::from(x) + 128) >> 8),
        Policy::Tpdf => {
            let a = (next_u32(state) >> 24) as u8;
            let b = (next_u32(state) >> 24) as u8;
            dither_quantize(x, a, b)
        }
    }
}

/// Approximate uniform/correlated pseudorandom streams, not mathematical independence.
/// q*256 lets the unchanged production encoder requantize the exact candidate PCM.
pub fn transform(input: &[i32], output: &mut [i32], policy: Policy, states: &mut [u32; 2]) {
    assert_eq!(input.len(), output.len());
    assert_eq!(input.len() % 2, 0);
    for (i, (&sample, out)) in input.iter().zip(output).enumerate() {
        *out = quantize(sample, policy, &mut states[i % 2]) * 256;
    }
}

/// Fixture generation is outside all target timing. Each phase repeats this source block.
pub fn fill_input(kind: &str, input: &mut [i32]) {
    assert_eq!(input.len() % 2, 0);
    let mut noise = [0xa341_316c, 0xc801_3ea4];
    let frames = input.len() / 2;
    let extremes = [
        i32::MIN,
        i32::MAX,
        -8_388_608,
        8_388_607,
        -129,
        -128,
        127,
        128,
    ];
    for (i, pair) in input.chunks_exact_mut(2).enumerate() {
        let values = match kind {
            "silence" => [0, 0],
            "quarter_dc" => [64, -64],
            "triangle_fade" => {
                // Integer triangle starts at 16 output LSB, then fades toward zero.
                let triangle = if i % 128 < 64 {
                    (i % 128) as i32 - 32
                } else {
                    96 - (i % 128) as i32
                };
                let x = triangle * 128 * (frames - i) as i32 / frames as i32;
                [x, -x]
            }
            "noise24" => [
                (next_u32(&mut noise[0]) as i32) >> 8,
                (next_u32(&mut noise[1]) as i32) >> 8,
            ],
            "extrema" => [
                extremes[i % extremes.len()],
                extremes[(i + 3) % extremes.len()],
            ],
            _ => panic!("unknown quant-cost fixture"),
        };
        pair.copy_from_slice(&values);
    }
}

pub fn hash_bytes(mut hash: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    hash
}

pub fn source_hash(input: &[i32]) -> u32 {
    input.iter().fold(HASH_INIT, |hash, sample| {
        hash_bytes(hash, &sample.to_le_bytes())
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Receipt {
    pub bytes: usize,
    pub hash: u32,
    pub pcm_hash: u32,
    pub largest: usize,
}

impl Receipt {
    pub fn new() -> Self {
        Self {
            hash: HASH_INIT,
            pcm_hash: HASH_INIT,
            ..Self::default()
        }
    }
    pub fn add_pcm(&mut self, transformed: &[i32]) {
        for &sample in transformed {
            self.pcm_hash = hash_bytes(self.pcm_hash, &((sample / 256) as i16).to_le_bytes());
        }
    }
    pub fn add(&mut self, frame: &[u8]) {
        self.bytes += frame.len();
        self.hash = hash_bytes(self.hash, frame);
        self.largest = self.largest.max(frame.len());
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Expected {
    pub receipt: Receipt,
    pub states: [u32; 2],
}
// Host-generated frame-number-zero receipts; [fixture][policy][warmup/measured].
pub const EXPECTED: [[[Expected; 2]; 3]; 5] = [
    [
        [
            Expected {
                receipt: Receipt {
                    bytes: 14,
                    hash: 73704201,
                    pcm_hash: 946445765,
                    largest: 14,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 224,
                    hash: 3640625349,
                    pcm_hash: 4125924805,
                    largest: 14,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 14,
                    hash: 73704201,
                    pcm_hash: 946445765,
                    largest: 14,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 224,
                    hash: 3640625349,
                    pcm_hash: 4125924805,
                    largest: 14,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 1418,
                    hash: 593320634,
                    pcm_hash: 574800257,
                    largest: 1418,
                },
                states: [2721297254, 742520434],
            },
            Expected {
                receipt: Receipt {
                    bytes: 22734,
                    hash: 346036681,
                    pcm_hash: 2065692654,
                    largest: 1431,
                },
                states: [934658367, 1507733665],
            },
        ],
    ],
    [
        [
            Expected {
                receipt: Receipt {
                    bytes: 14,
                    hash: 2356250528,
                    pcm_hash: 3187228101,
                    largest: 14,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 224,
                    hash: 3670890693,
                    pcm_hash: 1323736517,
                    largest: 14,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 14,
                    hash: 73704201,
                    pcm_hash: 946445765,
                    largest: 14,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 224,
                    hash: 3640625349,
                    pcm_hash: 4125924805,
                    largest: 14,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 1514,
                    hash: 3437193321,
                    pcm_hash: 3227935591,
                    largest: 1514,
                },
                states: [2721297254, 742520434],
            },
            Expected {
                receipt: Receipt {
                    bytes: 24290,
                    hash: 3104058593,
                    pcm_hash: 2780405793,
                    largest: 1531,
                },
                states: [934658367, 1507733665],
            },
        ],
    ],
    [
        [
            Expected {
                receipt: Receipt {
                    bytes: 1425,
                    hash: 855646451,
                    pcm_hash: 3989462504,
                    largest: 1425,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 22800,
                    hash: 1966949349,
                    pcm_hash: 4281873157,
                    largest: 1425,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 1425,
                    hash: 3769853680,
                    pcm_hash: 651848383,
                    largest: 1425,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 22800,
                    hash: 39075477,
                    pcm_hash: 1084957093,
                    largest: 1425,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 1870,
                    hash: 4150520757,
                    pcm_hash: 3005629134,
                    largest: 1870,
                },
                states: [2721297254, 742520434],
            },
            Expected {
                receipt: Receipt {
                    bytes: 29713,
                    hash: 775046515,
                    pcm_hash: 1990738189,
                    largest: 1873,
                },
                states: [934658367, 1507733665],
            },
        ],
    ],
    [
        [
            Expected {
                receipt: Receipt {
                    bytes: 16394,
                    hash: 3742402397,
                    pcm_hash: 2316935612,
                    largest: 16394,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 262304,
                    hash: 2190207301,
                    pcm_hash: 2175358405,
                    largest: 16394,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 16394,
                    hash: 2597917164,
                    pcm_hash: 3820589038,
                    largest: 16394,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 262304,
                    hash: 225074309,
                    pcm_hash: 4060703429,
                    largest: 16394,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 16394,
                    hash: 2610563896,
                    pcm_hash: 2412338754,
                    largest: 16394,
                },
                states: [2721297254, 742520434],
            },
            Expected {
                receipt: Receipt {
                    bytes: 262304,
                    hash: 824350108,
                    pcm_hash: 2099604048,
                    largest: 16394,
                },
                states: [934658367, 1507733665],
            },
        ],
    ],
    [
        [
            Expected {
                receipt: Receipt {
                    bytes: 16394,
                    hash: 386775797,
                    pcm_hash: 1189592517,
                    largest: 16394,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 262304,
                    hash: 4203222981,
                    pcm_hash: 3721305541,
                    largest: 16394,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 16394,
                    hash: 29500690,
                    pcm_hash: 504112581,
                    largest: 16394,
                },
                states: [305419896, 2596069105],
            },
            Expected {
                receipt: Receipt {
                    bytes: 262304,
                    hash: 2709307621,
                    pcm_hash: 269819333,
                    largest: 16394,
                },
                states: [305419896, 2596069105],
            },
        ],
        [
            Expected {
                receipt: Receipt {
                    bytes: 16394,
                    hash: 2985088324,
                    pcm_hash: 627057567,
                    largest: 16394,
                },
                states: [2721297254, 742520434],
            },
            Expected {
                receipt: Receipt {
                    bytes: 262304,
                    hash: 3975634661,
                    pcm_hash: 3584690624,
                    largest: 16394,
                },
                states: [934658367, 1507733665],
            },
        ],
    ],
];

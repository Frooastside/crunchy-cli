use lazy_static::lazy_static;
use regex::Regex;
use std::fmt;
use std::fmt::Formatter;
use std::str::FromStr;

pub const SOFTSUB_CONTAINERS: [&str; 3] = ["mkv", "mov", "mp4"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FFmpegPreset {
    Predefined(FFmpegCodec, Option<FFmpegHwAccel>, FFmpegQuality),
    Custom(Option<String>),
}

lazy_static! {
    static ref PREDEFINED_PRESET: Regex = Regex::new(r"^\w+(-\w+)*?$").unwrap();
}

macro_rules! ffmpeg_enum {
    (enum $name:ident { $($field:ident),* }) => {
        #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
        pub enum $name {
            $(
                $field
            ),*,
        }

        impl $name {
            fn all() -> Vec<$name> {
                vec![
                    $(
                        $name::$field
                    ),*,
                ]
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                match self {
                    $(
                        &$name::$field => write!(f, "{}", stringify!($field).to_string().to_lowercase())
                    ),*
                }
            }
        }

        impl FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                match s {
                    $(
                        stringify!($field) => Ok($name::$field)
                    ),*,
                    _ => anyhow::bail!("{} is not a valid {}", s, stringify!($name).to_lowercase())
                }
            }
        }
    }
}

ffmpeg_enum! {
    enum FFmpegCodec {
        H264,
        H265,
        Av1
    }
}

ffmpeg_enum! {
    enum FFmpegHwAccel {
        Nvidia,
        Amd,
        Apple
    }
}

ffmpeg_enum! {
    enum FFmpegQuality {
        Lossless,
        Normal,
        Low
    }
}

impl Default for FFmpegPreset {
    fn default() -> Self {
        Self::Custom(Some("-c:v copy -c:a copy".to_string()))
    }
}

impl FFmpegPreset {
    pub(crate) fn available_matches(
    ) -> Vec<(FFmpegCodec, Option<FFmpegHwAccel>, Option<FFmpegQuality>)> {
        let codecs = vec![
            (
                FFmpegCodec::H264,
                FFmpegHwAccel::all(),
                FFmpegQuality::all(),
            ),
            (
                FFmpegCodec::H265,
                FFmpegHwAccel::all(),
                FFmpegQuality::all(),
            ),
            (
                FFmpegCodec::Av1,
                vec![FFmpegHwAccel::Amd],
                FFmpegQuality::all(),
            ),
        ];

        let mut return_values = vec![];

        for (codec, hwaccels, qualities) in codecs {
            return_values.push((codec.clone(), None, None));
            for hwaccel in hwaccels.clone() {
                return_values.push((codec.clone(), Some(hwaccel), None));
            }
            for quality in qualities.clone() {
                return_values.push((codec.clone(), None, Some(quality)))
            }
            for hwaccel in hwaccels {
                for quality in qualities.clone() {
                    return_values.push((codec.clone(), Some(hwaccel.clone()), Some(quality)))
                }
            }
        }

        return_values
    }

    pub(crate) fn available_matches_human_readable() -> Vec<String> {
        let mut return_values = vec![];

        for (codec, hwaccel, quality) in FFmpegPreset::available_matches() {
            let mut description_details = vec![];
            if let Some(h) = &hwaccel {
                description_details.push(format!("{h} hardware acceleration"))
            }
            if let Some(q) = &quality {
                description_details.push(format!("{q} video quality/compression"))
            }

            let description = if description_details.is_empty() {
                format!("{codec} encoded with default video quality/compression",)
            } else if description_details.len() == 1 {
                format!("{} encoded with {}", codec, description_details[0])
            } else {
                let first = description_details.remove(0);
                let last = description_details.remove(description_details.len() - 1);
                let mid = if !description_details.is_empty() {
                    format!(", {} ", description_details.join(", "))
                } else {
                    "".to_string()
                };

                format!("{codec} encoded with {first}{mid} and {last}",)
            };

            return_values.push(format!(
                "{} ({})",
                vec![
                    Some(codec.to_string()),
                    hwaccel.map(|h| h.to_string()),
                    quality.map(|q| q.to_string())
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<String>>()
                .join("-"),
                description
            ))
        }
        return_values
    }

    pub(crate) fn parse(s: &str) -> Result<FFmpegPreset, String> {
        if !PREDEFINED_PRESET.is_match(s) {
            return Ok(FFmpegPreset::Custom(Some(s.to_string())));
        }

        let mut codec: Option<FFmpegCodec> = None;
        let mut hwaccel: Option<FFmpegHwAccel> = None;
        let mut quality: Option<FFmpegQuality> = None;
        for token in s.split('-') {
            if let Some(c) = FFmpegCodec::all()
                .into_iter()
                .find(|p| p.to_string() == token.to_lowercase())
            {
                if let Some(cc) = codec {
                    return Err(format!("cannot use multiple codecs (found {cc} and {c})",));
                }
                codec = Some(c)
            } else if let Some(h) = FFmpegHwAccel::all()
                .into_iter()
                .find(|p| p.to_string() == token.to_lowercase())
            {
                if let Some(hh) = hwaccel {
                    return Err(format!(
                        "cannot use multiple hardware accelerations (found {hh} and {h})",
                    ));
                }
                hwaccel = Some(h)
            } else if let Some(q) = FFmpegQuality::all()
                .into_iter()
                .find(|p| p.to_string() == token.to_lowercase())
            {
                if let Some(qq) = quality {
                    return Err(format!(
                        "cannot use multiple ffmpeg preset qualities (found {qq} and {q})",
                    ));
                }
                quality = Some(q)
            } else {
                return Err(format!(
                    "'{}' is not a valid ffmpeg preset (unknown token '{}')",
                    s, token
                ));
            }
        }

        if let Some(c) = codec {
            if !FFmpegPreset::available_matches().contains(&(
                c.clone(),
                hwaccel.clone(),
                quality.clone(),
            )) {
                return Err("ffmpeg preset is not supported".to_string());
            }
            Ok(FFmpegPreset::Predefined(
                c,
                hwaccel,
                quality.unwrap_or(FFmpegQuality::Normal),
            ))
        } else {
            Err("cannot use ffmpeg preset with without a codec".to_string())
        }
    }

    pub(crate) fn into_input_output_args(self) -> (Vec<String>, Vec<String>) {
        match self {
            FFmpegPreset::Custom(output) => (
                vec![],
                output.map_or(vec![], |o| shlex::split(&o).unwrap_or_default()),
            ),
            FFmpegPreset::Predefined(codec, hwaccel_opt, quality) => {
                let mut input = vec![];
                let mut output = vec![];

                match codec {
                    FFmpegCodec::H264 => {
                        let mut crf_quality = || match quality {
                            FFmpegQuality::Lossless => output.extend(["-crf", "18"]),
                            FFmpegQuality::Normal => (),
                            FFmpegQuality::Low => output.extend(["-crf", "35"]),
                        };

                        if let Some(hwaccel) = hwaccel_opt {
                            match hwaccel {
                                FFmpegHwAccel::Nvidia => {
                                    input.extend([
                                        "-hwaccel",
                                        "cuda",
                                        "-hwaccel_output_format",
                                        "cuda",
                                        "-c:v",
                                        "h264_cuvid",
                                    ]);
                                    crf_quality();
                                    output.extend(["-c:v", "h264_nvenc", "-c:a", "copy"])
                                }
                                FFmpegHwAccel::Amd => {
                                    crf_quality();
                                    output.extend(["-c:v", "h264_amf", "-c:a", "copy"])
                                }
                                FFmpegHwAccel::Apple => {
                                    // Apple's Video Toolbox encoders ignore `-crf`, use `-q:v`
                                    // instead. It's on a scale of 1-100, 100 being lossless. Just
                                    // did some math ((-a/51+1)*99+1 where `a` is the old crf value)
                                    // so these settings very likely need some more tweaking
                                    match quality {
                                        FFmpegQuality::Lossless => output.extend(["-q:v", "65"]),
                                        FFmpegQuality::Normal => (),
                                        FFmpegQuality::Low => output.extend(["-q:v", "32"]),
                                    }

                                    output.extend(["-c:v", "h264_videotoolbox", "-c:a", "copy"])
                                }
                            }
                        } else {
                            crf_quality();
                            output.extend(["-c:v", "libx264", "-c:a", "copy"])
                        }
                    }
                    FFmpegCodec::H265 => {
                        let mut crf_quality = || match quality {
                            FFmpegQuality::Lossless => output.extend(["-crf", "20"]),
                            FFmpegQuality::Normal => (),
                            FFmpegQuality::Low => output.extend(["-crf", "35"]),
                        };

                        if let Some(hwaccel) = hwaccel_opt {
                            match hwaccel {
                                FFmpegHwAccel::Nvidia => {
                                    input.extend([
                                        "-hwaccel",
                                        "cuda",
                                        "-hwaccel_output_format",
                                        "cuda",
                                        "-c:v",
                                        "h264_cuvid",
                                    ]);
                                    crf_quality();
                                    output.extend([
                                        "-c:v",
                                        "hevc_nvenc",
                                        "-c:a",
                                        "copy",
                                        "-tag:v",
                                        "hvc1",
                                    ])
                                }
                                FFmpegHwAccel::Amd => {
                                    crf_quality();
                                    output.extend(["-c:v", "hevc_amf", "-c:a", "copy"])
                                }
                                FFmpegHwAccel::Apple => {
                                    // See the comment for apple h264 hwaccel
                                    match quality {
                                        FFmpegQuality::Lossless => output.extend(["-q:v", "61"]),
                                        FFmpegQuality::Normal => (),
                                        FFmpegQuality::Low => output.extend(["-q:v", "32"]),
                                    }

                                    output.extend([
                                        "-c:v",
                                        "hevc_videotoolbox",
                                        "-c:a",
                                        "copy",
                                        "-tag:v",
                                        "hvc1",
                                    ])
                                }
                            }
                        } else {
                            crf_quality();
                            output.extend(["-c:v", "libx265", "-c:a", "copy", "-tag:v", "hvc1"])
                        }
                    }
                    FFmpegCodec::Av1 => {
                        let mut crf_quality = || match quality {
                            FFmpegQuality::Lossless => output.extend(["-crf", "22"]),
                            FFmpegQuality::Normal => (),
                            FFmpegQuality::Low => output.extend(["-crf", "35"]),
                        };

                        crf_quality();
                        if let Some(FFmpegHwAccel::Amd) = hwaccel_opt {
                            output.extend(["-c:v", "av1_amf", "-c:a", "copy"]);
                        } else {
                            output.extend(["-c:v", "libsvtav1", "-c:a", "copy"]);
                        }
                    }
                }

                (
                    input
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>(),
                    output
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>(),
                )
            }
        }
    }
}

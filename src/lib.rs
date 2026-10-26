//! Plan a target video length for Google's Veo 3.1 model.
//!
//! Two routes are compared, using the limits Google documents for developer
//! surfaces (checked 11 Oct 2026):
//!
//! * **Separate clips** — one clip is 4, 6 or 8 seconds (Gemini API and Vertex AI).
//!   [`separate_clips`] uses 8-second clips and counts one request per clip, because
//!   the Gemini API returns one video per request. [`min_surplus_mix`] finds a
//!   4/6/8-second mix with the least surplus.
//! * **Extension** — in the Gemini API a 720p Veo 3.1 / Veo 3.1 Fast video can be
//!   extended by 7 seconds up to 20 times, so the longest result is
//!   8 + 7 × 20 = 148 seconds. [`extension_plan`] returns the extension count,
//!   final length and request count (extensions + the first clip).
//!
//! Frame counts are arithmetic at [`FPS`] frames per second, not a published figure.
//! Status-polling calls are not requests here.
//!
//! The limits, their sources and a browser version of the planner:
//! <https://makeroll.com/veo-3-1-video-length-limit/>
//!
//! ```
//! use veo_clip_planner::{extension_plan, separate_clips};
//! let c = separate_clips(60).unwrap();
//! assert_eq!((c.clips, c.generated_seconds, c.requests), (8, 64, 8));
//! let e = extension_plan(60).unwrap();
//! assert_eq!((e.extensions, e.final_seconds, e.requests), (8, 64, 9));
//! ```

use std::fmt;

/// Clip lengths, in seconds, that one Veo 3.1 generation can return.
pub const CLIP_LENGTHS_S: [u32; 3] = [4, 6, 8];
/// Longest single clip, in seconds.
pub const MAX_CLIP_S: u32 = 8;
/// Seconds added by one extension.
pub const EXTENSION_S: u32 = 7;
/// Maximum number of extensions in the Gemini API.
pub const MAX_EXTENSIONS_GEMINI: u32 = 20;
/// Longest extended video in the Gemini API: 8 + 7 × 20.
pub const MAX_EXTENDED_GEMINI_S: u32 = MAX_CLIP_S + EXTENSION_S * MAX_EXTENSIONS_GEMINI;
/// Longest extended video stated for Vertex AI (different rules; informational only).
pub const MAX_EXTENDED_VERTEX_S: u32 = 37;
/// Frame rate used for frame-count arithmetic.
pub const FPS: u32 = 24;
/// Accepted target range, in whole seconds.
pub const TARGET_RANGE_S: (u32, u32) = (1, 3600);

/// Why a plan could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// Target outside [`TARGET_RANGE_S`].
    OutOfRange(u32),
    /// Target longer than [`MAX_EXTENDED_GEMINI_S`]; extension cannot reach it.
    ExceedsExtensionLimit(u32),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::OutOfRange(t) => write!(
                f,
                "target {t} s is outside {}..={} s",
                TARGET_RANGE_S.0, TARGET_RANGE_S.1
            ),
            PlanError::ExceedsExtensionLimit(t) => write!(
                f,
                "target {t} s exceeds the {MAX_EXTENDED_GEMINI_S} s Gemini API extension maximum"
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// Result of the separate-clips route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipPlan {
    /// Number of 8-second clips.
    pub clips: u32,
    /// Seconds actually generated (clips × 8).
    pub generated_seconds: u32,
    /// Generation requests (one per clip in the Gemini API).
    pub requests: u32,
    /// Frames at [`FPS`].
    pub frames: u32,
}

/// Result of the Gemini API extension route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionPlan {
    /// Number of 7-second extensions after the first 8-second clip.
    pub extensions: u32,
    /// Length of the single resulting video, in seconds.
    pub final_seconds: u32,
    /// Generation requests (first clip + extensions).
    pub requests: u32,
    /// Frames at [`FPS`].
    pub frames: u32,
}

fn check_range(target: u32) -> Result<(), PlanError> {
    if target < TARGET_RANGE_S.0 || target > TARGET_RANGE_S.1 {
        Err(PlanError::OutOfRange(target))
    } else {
        Ok(())
    }
}

/// Frames for a duration at [`FPS`].
pub fn frames(seconds: u32) -> u32 {
    seconds * FPS
}

/// Plan `target` seconds as separate 8-second clips.
pub fn separate_clips(target: u32) -> Result<ClipPlan, PlanError> {
    check_range(target)?;
    let clips = target.div_ceil(MAX_CLIP_S);
    let generated_seconds = clips * MAX_CLIP_S;
    Ok(ClipPlan {
        clips,
        generated_seconds,
        requests: clips,
        frames: frames(generated_seconds),
    })
}

/// Plan `target` seconds as one 8-second clip plus 7-second Gemini API extensions.
pub fn extension_plan(target: u32) -> Result<ExtensionPlan, PlanError> {
    check_range(target)?;
    if target > MAX_EXTENDED_GEMINI_S {
        return Err(PlanError::ExceedsExtensionLimit(target));
    }
    let extensions = target.saturating_sub(MAX_CLIP_S).div_ceil(EXTENSION_S);
    let final_seconds = MAX_CLIP_S + extensions * EXTENSION_S;
    Ok(ExtensionPlan {
        extensions,
        final_seconds,
        requests: extensions + 1,
        frames: frames(final_seconds),
    })
}

/// Smallest-surplus mix of 4, 6 and 8-second clips covering `target` seconds.
///
/// Ties on total length are broken by the fewest clips (fewest requests).
/// Clips are returned longest first, e.g. `30 -> [8, 8, 8, 6]`.
pub fn min_surplus_mix(target: u32) -> Result<Vec<u32>, PlanError> {
    check_range(target)?;
    // Every even total >= 4 is reachable; search totals from target upward.
    let limit = target + MAX_CLIP_S;
    // best[n] = minimal clip count to make exactly n seconds, with the last clip used.
    let size = (limit + 1) as usize;
    let mut best: Vec<Option<(u32, u32)>> = vec![None; size];
    best[0] = Some((0, 0));
    for n in 1..size {
        for &c in CLIP_LENGTHS_S.iter() {
            let c_us = c as usize;
            if n >= c_us {
                if let Some((count, _)) = best[n - c_us] {
                    let cand = (count + 1, c);
                    if best[n].map_or(true, |(b, _)| cand.0 < b) {
                        best[n] = Some(cand);
                    }
                }
            }
        }
    }
    let mut total = target as usize;
    while best[total].is_none() {
        total += 1;
    }
    let mut out = Vec::new();
    let mut n = total;
    while n > 0 {
        let (_, c) = best[n].expect("reachable");
        out.push(c);
        n -= c as usize;
    }
    out.sort_unstable_by(|a, b| b.cmp(a));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_table_separate_clips() {
        assert_eq!(separate_clips(30).unwrap(), ClipPlan { clips: 4, generated_seconds: 32, requests: 4, frames: 768 });
        assert_eq!(separate_clips(60).unwrap().clips, 8);
        let c = separate_clips(148).unwrap();
        assert_eq!((c.clips, c.generated_seconds, c.requests), (19, 152, 19));
        assert_eq!(separate_clips(149).unwrap().generated_seconds, 152);
    }

    #[test]
    fn page_table_extension() {
        let e = extension_plan(30).unwrap();
        assert_eq!((e.extensions, e.final_seconds, e.requests), (4, 36, 5));
        let e = extension_plan(60).unwrap();
        assert_eq!((e.extensions, e.final_seconds, e.requests), (8, 64, 9));
        let e = extension_plan(148).unwrap();
        assert_eq!((e.extensions, e.final_seconds, e.requests, e.frames), (20, 148, 21, 3552));
        assert_eq!(extension_plan(149), Err(PlanError::ExceedsExtensionLimit(149)));
    }

    #[test]
    fn short_targets_need_no_extension() {
        for t in 1..=8 {
            let e = extension_plan(t).unwrap();
            assert_eq!((e.extensions, e.final_seconds, e.requests, e.frames), (0, 8, 1, 192));
        }
    }

    #[test]
    fn range_is_enforced() {
        assert_eq!(separate_clips(0), Err(PlanError::OutOfRange(0)));
        assert_eq!(separate_clips(3601), Err(PlanError::OutOfRange(3601)));
        assert!(separate_clips(3600).is_ok());
    }

    #[test]
    fn mix_matches_page_example() {
        assert_eq!(min_surplus_mix(30).unwrap(), vec![8, 8, 8, 6]);
        assert_eq!(min_surplus_mix(4).unwrap(), vec![4]);
        assert_eq!(min_surplus_mix(1).unwrap(), vec![4]);
        assert_eq!(min_surplus_mix(10).unwrap(), vec![6, 4]);
        assert_eq!(min_surplus_mix(16).unwrap(), vec![8, 8]);
    }

    #[test]
    fn mix_never_worse_than_eight_second_clips() {
        for t in 1..=400 {
            let mix: u32 = min_surplus_mix(t).unwrap().iter().sum();
            assert!(mix >= t);
            assert!(mix <= separate_clips(t).unwrap().generated_seconds);
        }
    }

    #[test]
    fn constants() {
        assert_eq!(MAX_EXTENDED_GEMINI_S, 148);
        assert_eq!(frames(8), 192);
    }
}

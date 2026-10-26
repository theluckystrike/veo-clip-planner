# veo-clip-planner

Small, dependency-free Rust library that answers one planning question for
Google's Veo 3.1 video model: *how do I reach a target length, and what does it
cost in requests?*

It compares the two routes that the developer documentation allows:

1. **Separate clips.** One Veo 3.1 clip is 4, 6 or 8 seconds long (Gemini API
   and Vertex AI). The planner uses 8-second clips: `clips = ceil(target / 8)`,
   one request per clip (the Gemini API returns one video per request).
   `min_surplus_mix` also finds a 4/6/8 mix with the least surplus, e.g.
   30 s = 8 + 8 + 8 + 6.
2. **Extension (Gemini API).** One 8-second 720p clip, then 7-second
   extensions, at most 20 of them, so the longest result is
   8 + 7 x 20 = 148 seconds. `extensions = ceil((target - 8) / 7)`,
   requests = extensions + 1. Vertex AI states different extension limits
   (37 s maximum) and is exposed only as a constant.

Frame counts are plain arithmetic at 24 fps (8 s = 192 frames), not a number
published by Google. Status-polling calls are not counted as requests.

```rust
use veo_clip_planner::{separate_clips, extension_plan};

let clips = separate_clips(30).unwrap();
assert_eq!((clips.clips, clips.generated_seconds, clips.requests), (4, 32, 4));

let ext = extension_plan(30).unwrap();
assert_eq!((ext.extensions, ext.final_seconds, ext.requests), (4, 36, 5));

assert!(extension_plan(149).is_err()); // over the 148 s Gemini API ceiling
```

| Target | Separate 8 s clips | Gemini API extension |
|-------:|--------------------|----------------------|
| 30 s   | 4 clips, 32 s, 4 requests | 4 extensions, 36 s, 5 requests |
| 60 s   | 8 clips, 64 s, 8 requests | 8 extensions, 64 s, 9 requests |
| 148 s  | 19 clips, 152 s, 19 requests | 20 extensions, 148 s, 21 requests |
| 149 s  | 19 clips, 152 s, 19 requests | not possible |

The limits with their primary sources (the ai.google.dev page "Generate videos
with Veo 3.1 in Gemini API" and Google's Vertex AI documentation, read on
11 Oct 2026) and an in-browser version of this planner are on MakeRoll's
[Veo 3.1 video length limit page](https://makeroll.com/veo-3-1-video-length-limit/).
Limits change; check the official pages before relying on these constants.

License: MIT

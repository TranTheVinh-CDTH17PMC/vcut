// Kiểu dữ liệu khớp với crates/core/src/project.rs (dạng JSON).

export type Easing = "linear" | "ease_in" | "ease_out" | "ease_in_out" | "hold";
export interface Keyframe<T> {
  t: number;
  value: T;
  easing?: Easing;
}
/** Giá trị cố định hoặc danh sách keyframe. */
export type Animatable<T> = T | Keyframe<T>[];
export interface Vec2 {
  x: number;
  y: number;
}

export interface Settings {
  width: number;
  height: number;
  fps: number;
}

export type MediaKind = "video" | "image" | "audio";
export type SceneTag = "good" | "blurry" | "dark" | "overexposed" | "shaky" | "duplicate" | "speech" | "photo";
export type SceneMark = "keep" | "drop";
export interface Scene {
  id: string;
  start: number;
  end: number;
  shot: number;
  sharpness?: number | null;
  brightness?: number | null;
  motion?: number | null;
  shake?: number | null;
  audio_db?: number | null;
  thumb?: string | null;
  tags: SceneTag[];
  score: number;
  dup_of?: string | null;
  mark?: SceneMark | null;
}

export interface Brief {
  target_secs: number | null;
  width: number;
  height: number;
  mood: string;
  music_media_id: string | null;
  chronological: boolean;
  place_titles: boolean;
  keep_speech: boolean;
  beat_sync: boolean;
  notes: string;
}

export interface MediaItem {
  id: string;
  path: string;
  name: string;
  kind: MediaKind;
  duration: number;
  width: number;
  height: number;
  fps: number;
  has_audio: boolean;
  created_at?: string | null;
  thumb?: string | null;
  proxy?: string | null;
  scenes: Scene[];
  analysis_version: number;
  modified_at?: string | null;
  gps?: { lat: number; lon: number } | null;
  transcribed?: boolean;
}

export type FilterPreset = "none" | "cinematic" | "vintage" | "cool" | "warm" | "black_white" | "vivid";
/** id trong bảng chuyển cảnh (crates/core/src/transitions.rs), vd. "fade", "spin". */
export type TransitionKind = string;

export interface TransitionInfo {
  id: string;
  name: string;
  group: string;
}

export interface Transition {
  kind: TransitionKind;
  duration: number;
}

export interface VideoClip {
  id: string;
  media_id: string;
  src_in: number;
  src_out: number;
  speed: number;
  color: {
    preset: FilterPreset;
    brightness: Animatable<number>;
    contrast: Animatable<number>;
    saturation: Animatable<number>;
  };
  transform: {
    position: Animatable<Vec2>;
    scale: Animatable<number>;
    rotation: Animatable<number>;
    opacity: Animatable<number>;
  };
  motion_blur: number;
  transition_in?: Transition | null;
  volume: Animatable<number>;
}

export type TextAlign = "top" | "center" | "bottom";
export type TextAnim = "none" | "fade" | "pop" | "slide_up";
export interface TextStyle {
  font: string;
  size: number;
  color: string;
  outline_color: string;
  outline: number;
  bold: boolean;
  align: TextAlign;
  anim_in: TextAnim;
  karaoke?: boolean;
  highlight_color?: string;
}
export interface TextWord {
  text: string;
  start: number;
  end: number;
}
export interface TextClip {
  id: string;
  text: string;
  start: number;
  end: number;
  style: TextStyle;
  transform: {
    position?: Animatable<Vec2> | null;
    scale?: Animatable<number> | null;
    opacity?: Animatable<number> | null;
  };
  words?: TextWord[];
}

export type AudioRole = "music" | "voice";
export interface AudioClip {
  id: string;
  media_id: string;
  role: AudioRole;
  start: number;
  src_in: number;
  src_out: number;
  volume: Animatable<number>;
  fade_in: number;
  fade_out: number;
  duck: boolean;
}

export interface Project {
  version: number;
  name: string;
  revision: number;
  settings: Settings;
  media: MediaItem[];
  video: VideoClip[];
  texts: TextClip[];
  audio: AudioClip[];
}

export interface Diff {
  lines: string[];
  affected: string[];
  duration_before: number;
  duration_after: number;
}

export interface Proposal {
  id: string;
  description: string;
  notes?: string | null;
  base_revision: number;
  ops: Op[];
  diff: Diff;
}

export interface ProjectView {
  dir: string;
  project: Project;
  can_undo: boolean;
  can_redo: boolean;
  proposal?: Proposal | null;
  claude_message?: ClaudeMessage | null;
}

export interface ClaudeMessage {
  id: string;
  text: string;
  at: string;
}

/** Một thao tác sửa timeline, vd. { op: "add_clip", media_id: "..." }. */
export type Op = { op: string } & Record<string, unknown>;

export interface PresetInfo {
  id: FilterPreset;
  label: string;
  css: string;
}

export interface AppInfo {
  ffmpeg: { found: boolean; version: string; encoder: "nvenc" | "x264" };
  presets: PresetInfo[];
  transitions: TransitionInfo[];
  transition_groups: string[];
  last_project?: string | null;
}

export type Selection = { kind: "clip" | "text" | "audio"; id: string } | null;

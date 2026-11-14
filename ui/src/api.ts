// Gọi lệnh Rust (Tauri command) và nghe sự kiện.

import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppInfo, Brief, Op, ProjectView, SceneMark } from "./types";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  open: (dir: string) => invoke<ProjectView>("project_open", { dir }),
  get: () => invoke<ProjectView>("project_get"),
  apply: (ops: Op[], expectedRevision?: number) =>
    invoke<ProjectView>("project_apply", { ops, expectedRevision }),
  undo: () => invoke<ProjectView>("project_undo"),
  redo: () => invoke<ProjectView>("project_redo"),
  acceptProposal: () => invoke<ProjectView>("proposal_accept"),
  dismissProposal: () => invoke<ProjectView>("proposal_dismiss"),
  importMedia: (paths: string[]) => invoke<void>("media_import", { paths }),
  removeMedia: (mediaId: string) => invoke<ProjectView>("media_remove", { mediaId }),
  exportStart: (outPath: string) => invoke<void>("export_start", { outPath }),
  exportCancel: () => invoke<void>("export_cancel"),
  previewClip: (clipId: string) => invoke<string>("preview_clip", { clipId }),
  analysisStart: (force = false) => invoke<void>("analysis_start", { force }),
  analysisCancel: () => invoke<void>("analysis_cancel"),
  sceneMark: (sceneId: string, mark: SceneMark | null) => invoke<ProjectView>("scene_mark", { sceneId, mark }),
  briefGet: () => invoke<Brief>("brief_get"),
  briefSave: (brief: Brief) => invoke<void>("brief_save", { brief }),
  autoeditQuick: (brief: Brief) => invoke<ProjectView>("autoedit_quick", { brief }),
  claudeStatus: () => invoke<ClaudeStatus>("claude_status"),
  claudeConnect: () => invoke<ClaudeStatus>("claude_connect"),
  claudeOpen: () => invoke<boolean>("claude_open"),
  claudeMessageDismiss: () => invoke<ProjectView>("claude_message_dismiss"),
  speechStatus: () => invoke<SpeechStatus>("speech_status"),
  speechInstall: () => invoke<void>("speech_install"),
  speechCancel: () => invoke<void>("speech_cancel"),
  speechTranscribe: (mediaIds: string[] | null, language: string, force = false) =>
    invoke<void>("speech_transcribe", { mediaIds, language, force }),
  speechCleanup: (opts: CleanupOptions) => invoke<ProjectView>("speech_cleanup", { opts }),
  speechSubtitles: (opts: SubtitleOptions) => invoke<ProjectView>("speech_subtitles", { opts }),
};

export interface ClaudeStatus {
  configured: boolean;
  config_path: string;
  mcp_found: boolean;
  last_call?: string | null;
  last_tool?: string | null;
}

export interface AnalysisProgress {
  done: number;
  total: number;
  name: string;
}

export { listen };

/** Đường dẫn file trên máy -> URL mà webview phát được. */
export const fileUrl = (path?: string | null) => (path ? convertFileSrc(path) : "");

export interface ImportProgress {
  phase: "scan" | "proxy";
  done: number;
  total: number;
  name: string;
}
export interface ExportDone {
  ok: boolean;
  path: string;
  error?: string | null;
  cancelled: boolean;
}

export interface SpeechStatus {
  tools: { cli?: string | null; model?: string | null; gpu: boolean };
  nvidia: boolean;
  installing: boolean;
  transcribing: boolean;
}
export interface CleanupOptions {
  remove_fillers: boolean;
  max_pause: number | null;
  remove_words: string[];
  clip_ids: string[];
}
export interface SubtitleOptions {
  karaoke: boolean;
  align: "top" | "center" | "bottom";
  size: number | null;
  highlight_color: string;
  color: string;
  max_chars: number | null;
  hide_fillers: boolean;
}
export interface InstallProgress {
  phase: "whisper" | "model" | "extract";
  done: number;
  total: number;
}
export interface TranscribeProgress {
  done: number;
  total: number;
  name: string;
  pct: number;
}

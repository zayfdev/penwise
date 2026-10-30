/**
 * The M2 additions of the wasm `Viewer` (contract K1), named once for the client.
 *
 * Declared here instead of imported from `generated/` so the PC window builds against the contract while the
 * wasm package lags a build behind; there is no logic, only the signatures.
 */

import type { Viewer } from "../wasm";

/** Frames, layouts and marks as the core exposes them (docs/design-harness-spec.md §6.3, contract K1). */
export interface CoreApi {
  /** Merges a frame's layout; true when what is drawn may have changed. */
  set_frame_layout(frame: string, json: string): boolean;
  /** Drops a frame's layout entirely (demote to the still: the next `set_frame_layout` replaces, never merges). */
  clear_frame_layout(frame: string): boolean;
  /** JSON array of element keys the frame's bound strokes need rects for. */
  frame_keys(frame: string): string;
  /** Commits the open stroke with a binding: `{"frame","mode","key"?}`. */
  stroke_end_bound(now_ms: number, binding_json: string): void;
  /** JSON array of the page's marks; `rect` is effective page coords, `null` when hidden. */
  marks(page: string): string;
  /** Sets a mark's status (an undoable LWW edit), queueing the op for the next `Connection.flush`. */
  set_mark_status(id: string, status: string, now_ms: number): void;
  /** Shows or hides done marks; true when what is drawn may have changed. */
  set_show_done(on: boolean): boolean;
  /** M4 (contract P4): the owner's pick among a variant group; one batch, one undo step. */
  set_frame_chosen(id: string, chosen: boolean, now_ms: number): void;
  /** V2 (contract §2.2): `[x, y, w, h]` of the frame's live window in board coords; empty for an unknown frame. */
  frame_live_window(frame: string): Float32Array;
  /** V2: the document point `[x, y]` under a board point; empty for an unknown frame. */
  frame_doc_point(frame: string, x: number, y: number): Float32Array;
  /** V2: the board point `[x, y]` of a document point (inverse of `frame_doc_point`). */
  frame_board_point(frame: string, x: number, y: number): Float32Array;
}

/** The viewer seen through the M2 contract. */
export function coreApi(viewer: Viewer): CoreApi {
  return viewer as unknown as CoreApi;
}

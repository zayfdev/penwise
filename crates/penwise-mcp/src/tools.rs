//! The four tool definitions (spec v2 §5), kept as JSON so the schemas read like the protocol docs.

pub(crate) fn tools_list() -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(TOOLS)
        .unwrap_or_else(|_| serde_json::json!({ "tools": [] }))
}

/// Tool definitions. Short on purpose: every word is paid for on every request.
const TOOLS: &str = r##"{
  "tools": [
    {
      "name": "look",
      "description": "Read the owner's marks on the board. Returns compact text (project, board, round, frames, one line per open mark: kind, colour meaning, handwriting, selector, file:line, key styles; the owner's variant picks) and one numbered overview image per frame. Returned marks move new -> seen. Takes the latest sent round, else the open one.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "mark": { "type": "string", "description": "A mark id: its full bundle and crops." },
          "frame": { "type": "string", "description": "Only the marks on this frame id." },
          "images": { "type": "string", "enum": ["overview", "none"], "description": "overview (default) or none." }
        }
      }
    },
    {
      "name": "show",
      "description": "Show the owner something, exactly one of: frame (a route of the project on the board), ghost (a temporary patch on a frame), variants (2-4 patched copies beside a frame), sketch (strokes on the agent layer), image (a local PNG), focus (move the owner's view). The owner's device follows the result.",
      "inputSchema": {
        "type": "object",
        "minProperties": 1,
        "maxProperties": 1,
        "properties": {
          "frame": {
            "type": "object",
            "description": "Open a route as a frame; an existing frame with the same route and preset is reused.",
            "properties": {
              "route": { "type": "string", "description": "Path and query, starting with /." },
              "preset": { "type": "string", "enum": ["desktop", "tablet", "phone"], "description": "Default desktop." },
              "extent": { "type": "string", "enum": ["full", "viewport"], "description": "Default full (the whole page)." }
            },
            "required": ["route"]
          },
          "ghost": {
            "type": "object",
            "description": "Show a patch live on a frame, or clear it. patch = {css?, dom?: [{op: set_text|set_html|set_attr|remove|insert|replace, selector, value|html|name}]}.",
            "properties": {
              "frame": { "type": "string", "description": "Frame id." },
              "patch": { "type": "object" },
              "clear": { "type": "boolean" }
            },
            "required": ["frame"]
          },
          "variants": {
            "type": "object",
            "description": "Create 2-4 variants beside a frame for the owner to pick from, or remove them.",
            "properties": {
              "frame": { "type": "string", "description": "Frame id." },
              "options": {
                "type": "array",
                "minItems": 2,
                "maxItems": 4,
                "items": {
                  "type": "object",
                  "properties": { "label": { "type": "string" }, "patch": { "type": "object" } },
                  "required": ["label", "patch"]
                }
              },
              "clear": { "type": "boolean" }
            },
            "required": ["frame"]
          },
          "sketch": {
            "type": "object",
            "description": "Draw polylines on the board (agent layer).",
            "properties": {
              "strokes": {
                "type": "array",
                "items": {
                  "type": "object",
                  "properties": {
                    "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" } }, "description": "[[x, y], ...] in px." },
                    "color": { "type": "string", "description": "#RRGGBB" },
                    "width": { "type": "number" }
                  },
                  "required": ["points"]
                }
              },
              "near": { "type": "string", "description": "Frame id: points are then relative to its top-left corner." }
            },
            "required": ["strokes"]
          },
          "image": {
            "type": "object",
            "description": "Place a local PNG on the board.",
            "properties": {
              "path": { "type": "string", "description": "Local PNG file." },
              "x": { "type": "number" },
              "y": { "type": "number" },
              "label": { "type": "string" }
            },
            "required": ["path"]
          },
          "focus": {
            "type": "object",
            "description": "Move the owner's view to a frame or a page.",
            "properties": {
              "frame": { "type": "string", "description": "Frame id." },
              "page": { "type": "string", "description": "Page id." }
            }
          }
        }
      }
    },
    {
      "name": "resolve",
      "description": "Report what you did. marks: set each mark done, reopened or seen. round: n closes round n, the number look shows (the frame freezes into history and moves right; ghosts and variants clear). true means the latest sent round; an open round needs its number. The round also closes when its last open mark is done.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "marks": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "id": { "type": "string" },
                "status": { "type": "string", "enum": ["done", "reopened", "seen"] },
                "note": { "type": "string" }
              },
              "required": ["id", "status"]
            }
          },
          "round": { "type": "integer", "minimum": 1, "description": "The round number from look." }
        }
      }
    },
    {
      "name": "notes",
      "description": "The owner's plain notes (not the design board): list pages, read handwriting as text, search, or get a page as PNG.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": { "type": "string", "description": "Search the pages already read." },
          "page": { "type": "string", "description": "Page id (default: the active page)." },
          "png": { "type": "boolean", "description": "Return the page as a PNG instead of text." }
        }
      }
    }
  ]
}"##;

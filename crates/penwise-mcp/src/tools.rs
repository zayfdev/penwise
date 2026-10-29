//! Tool definitions, kept as JSON so the schemas read like the protocol docs.

pub(crate) fn tools_list() -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(TOOLS)
        .unwrap_or_else(|_| serde_json::json!({ "tools": [] }))
}

/// Tool definitions, kept as JSON so the schemas read like the protocol docs.
const TOOLS: &str = r##"{
  "tools": [
    {
      "name": "penwise_page_png",
      "description": "Render a Penwise page (the user's handwriting/drawing) as a PNG image.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "scale": { "type": "number", "description": "Pixels per page pixel, 0.05–4 (default 1; 0.5 for a smaller image)." },
          "page": { "type": "string", "description": "Page id; defaults to the active page." }
        }
      }
    },
    {
      "name": "penwise_page_status",
      "description": "Stroke and page counts for the active page.",
      "inputSchema": {
        "type": "object",
        "properties": { "page": { "type": "string", "description": "Page id; defaults to the active page." } }
      }
    },
    {
      "name": "penwise_annotate",
      "description": "Draw polylines on the user's page (marks appear as a separate agent layer and can be cleared).",
      "inputSchema": {
        "type": "object",
        "properties": {
          "page": { "type": "string", "description": "Page id; defaults to the active page." },
          "strokes": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "points": {
                  "type": "array",
                  "description": "Page-pixel points [[x, y], …]; two or more make a line.",
                  "items": { "type": "array", "items": { "type": "number" } }
                },
                "color": { "type": "string", "description": "#RRGGBB or #AARRGGBB (default red)" },
                "width": { "type": "number", "description": "Stroke width in page px (default 3)" }
              },
              "required": ["points"]
            }
          }
        },
        "required": ["strokes"]
      }
    },
    {
      "name": "penwise_clear_agent_marks",
      "description": "Remove every mark this or another agent drew on the page (the user's own ink is untouched).",
      "inputSchema": {
        "type": "object",
        "properties": { "page": { "type": "string", "description": "Page id; defaults to the active page." } }
      }
    },
    {
      "name": "penwise_place_image",
      "description": "Place a picture on the page (a screenshot, a component cut-out, a reference). Ink draws over it and page PNGs include it.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "Local PNG file to upload and place." },
          "asset": { "type": "string", "description": "An already-uploaded asset id, instead of path." },
          "x": { "type": "number", "description": "Left edge in page px." },
          "y": { "type": "number", "description": "Top edge in page px." },
          "w": { "type": "number", "description": "Width in page px; defaults to the picture's own width." },
          "h": { "type": "number", "description": "Height in page px; defaults to the picture's own height." },
          "label": { "type": "string", "description": "What it is: a URL, a component name." },
          "page": { "type": "string", "description": "Page id; defaults to the active page." }
        },
        "required": ["x", "y"]
      }
    },
    {
      "name": "penwise_list_images",
      "description": "The pictures placed on a page, with their ids, assets, positions and labels.",
      "inputSchema": {
        "type": "object",
        "properties": { "page": { "type": "string", "description": "Page id; defaults to the active page." } }
      }
    },
    {
      "name": "penwise_remove_image",
      "description": "Remove one placed picture by id.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "id": { "type": "string", "description": "Picture id from penwise_list_images." },
          "page": { "type": "string", "description": "Page id; defaults to the active page." }
        },
        "required": ["id"]
      }
    },
    {
      "name": "penwise_page_annotations",
      "description": "Which elements of a captured page the ink belongs to: per-selector stroke counts, plus how many strokes fall outside every element.",
      "inputSchema": {
        "type": "object",
        "properties": { "page": { "type": "string", "description": "Page id; defaults to the active page." } }
      }
    },
    {
      "name": "penwise_list_pages",
      "description": "List notebooks and their pages, with the active page id.",
      "inputSchema": { "type": "object", "properties": {} }
    },
    {
      "name": "penwise_create_page",
      "description": "Create a page (defaults: next free title, A4 portrait, blank).",
      "inputSchema": {
        "type": "object",
        "properties": {
          "title": { "type": "string" },
          "background": { "type": "string", "enum": ["blank", "ruled", "grid", "dots"] },
          "kind": { "type": "object", "description": "{\"t\":\"fixed\",\"width\":W,\"height\":H} or {\"t\":\"infinite\"}" },
          "notebook": { "type": "string" }
        }
      }
    },
    {
      "name": "penwise_page_text",
      "description": "Handwriting on a page as text, read by a local vision model (cached until refresh).",
      "inputSchema": {
        "type": "object",
        "properties": {
          "page": { "type": "string", "description": "Page id; defaults to the active page." },
          "refresh": { "type": "boolean", "description": "Re-read the page instead of returning the cache." }
        }
      }
    },
    {
      "name": "penwise_search",
      "description": "Search the handwriting of every page already read as text.",
      "inputSchema": {
        "type": "object",
        "properties": { "query": { "type": "string", "description": "Words to look for, case-insensitive." } },
        "required": ["query"]
      }
    },
    {
      "name": "penwise_create_notebook",
      "description": "Create an empty notebook to group pages.",
      "inputSchema": {
        "type": "object",
        "properties": { "title": { "type": "string" } },
        "required": ["title"]
      }
    },
    {
      "name": "penwise_switch_page",
      "description": "Make a page the active one for the HTTP tools and tell the tablet and browser windows to follow.",
      "inputSchema": {
        "type": "object",
        "properties": { "page": { "type": "string" } },
        "required": ["page"]
      }
    },
    {
      "name": "penwise_projects",
      "description": "Linked design projects: id, name, root, kind (static or proxy), dev origin, port, notebook. Use the id in the frame and mark tools.",
      "inputSchema": { "type": "object", "properties": {} }
    },
    {
      "name": "penwise_frame_open",
      "description": "Open a project route as a live frame on a page. Without `page`, the project's \"<name> review\" notebook page is used (created if missing) and the frame goes right of the existing ones.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "project": { "type": "string", "description": "Project id from penwise_projects." },
          "route": { "type": "string", "description": "Same-origin path on the project, e.g. \"/\" or \"/work?tag=one\"." },
          "preset": { "type": "string", "enum": ["phone", "tablet", "desktop", "custom"], "description": "Frame size preset (phone 390×844, tablet 834×1194, desktop 1440×900)." },
          "page": { "type": "string", "description": "Page id; defaults to the project's review page." }
        },
        "required": ["project", "route", "preset"]
      }
    },
    {
      "name": "penwise_frame_snapshot",
      "description": "PNG of a frame's live page at its size preset; `ink` includes the owner's marks (default true).",
      "inputSchema": {
        "type": "object",
        "properties": {
          "frame": { "type": "string", "description": "Frame item id." },
          "ink": { "type": "boolean", "description": "Include the owner's ink (default true)." }
        },
        "required": ["frame"]
      }
    },
    {
      "name": "penwise_marks",
      "description": "Mark bundles for what the owner drew on a frame: kind guess (from/to elements for arrows), colour meaning, OCR text, the element (selector, role, text, styles, rect, source file:line rules) and crop images. Advances the agent's checkpoint.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "project": { "type": "string", "description": "Only marks on this project's frames." },
          "page": { "type": "string", "description": "Only marks on this page." },
          "frame": { "type": "string", "description": "Only marks on this frame." },
          "status": { "type": "string", "enum": ["new", "seen", "done", "reopened", "all"], "description": "Status filter; the default is new plus reopened." },
          "since": { "type": "string", "description": "\"last\" (default) = created or changed since this agent's previous check; \"none\" = no checkpoint filter." },
          "agent": { "type": "string", "description": "Checkpoint name, so several agents each see their own new marks (default \"agent\")." },
          "images": { "type": "string", "enum": ["none", "with_ink", "all"], "description": "Image blocks to return: with_ink (default), all (with_ink + context), or none." }
        }
      }
    },
    {
      "name": "penwise_mark_status",
      "description": "Set a mark's status: seen after reading it, done after the edit landed, reopened when it needs another look.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "id": { "type": "string", "description": "Mark id from penwise_marks." },
          "status": { "type": "string", "enum": ["seen", "done", "reopened", "new"] },
          "note": { "type": "string", "description": "Optional note for the owner." }
        },
        "required": ["id", "status"]
      }
    }
  ]
}"##;

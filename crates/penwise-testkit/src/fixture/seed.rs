//! The rows of the v3 fixture.

use rusqlite::{Connection, params};

use super::{Expected, ImageExpect, StrokeExpect, stroke_blob, wave};

/// One `objects` row.
struct ObjectRow<'a> {
    page: &'a str,
    id: &'a str,
    kind: &'a str,
    layer: &'a str,
    z: i64,
    codec: &'a str,
    payload: &'a [u8],
}

/// One decodable stroke row plus what the migration must produce for it.
/// `(page, id, layer, z, argb, scale, blob seed, sample count, expected seed)`.
type StrokeRow = (
    &'static str,
    &'static str,
    &'static str,
    i64,
    i32,
    f32,
    u64,
    usize,
    Option<u64>,
);

/// `(page, id, kind, z, codec, payload)` of a row the migration must quarantine.
type BadRow<'a> = (&'a str, &'a str, &'a str, i64, &'a str, &'a [u8]);

struct StrokeSpec<'a> {
    page: &'a str,
    id: &'a str,
    layer: &'a str,
    z: i64,
    argb: i32,
    scale: f32,
    blob_seed: u64,
    samples: usize,
    seed: Option<u64>,
}

pub(super) struct Rows<'a> {
    pub(super) conn: &'a Connection,
    pub(super) expect: Expected,
}

pub(super) fn err(e: &rusqlite::Error) -> String {
    e.to_string()
}

impl<'a> Rows<'a> {
    pub(super) fn new(conn: &'a Connection) -> Self {
        Self {
            conn,
            expect: Expected::default(),
        }
    }

    fn object(&mut self, row: &ObjectRow<'_>) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO objects (page, id, kind, layer, z, codec, payload) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![row.page, row.id, row.kind, row.layer, row.z, row.codec, row.payload],
            )
            .map_err(|e| err(&e))?;
        self.expect.objects_rows += 1;
        Ok(())
    }

    fn mark(&mut self, page: &str, migrated: bool) {
        if let Some(p) = self.expect.pages.iter_mut().find(|p| p.id == page) {
            if migrated {
                p.migrated += 1;
            } else {
                p.unmigrated += 1;
            }
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "phase only decorrelates the waves"
    )]
    fn stroke(&mut self, spec: &StrokeSpec<'_>) -> Result<(), String> {
        let (samples, times) = wave(spec.samples, (spec.z % 1000) as f32 * 1e-2);
        let blob = stroke_blob(3, spec.argb, spec.scale, spec.blob_seed, &samples, &times);
        self.object(&ObjectRow {
            page: spec.page,
            id: spec.id,
            kind: "stroke",
            layer: spec.layer,
            z: spec.z,
            codec: "stroke_v1",
            payload: &blob,
        })?;
        self.expect.strokes.push(StrokeExpect {
            page: spec.page.to_owned(),
            row_id: spec.id.to_owned(),
            argb: spec.argb,
            z: spec.z,
            agent: spec.layer == "agent",
            samples: spec.samples,
            seed: spec.seed,
        });
        self.mark(spec.page, true);
        Ok(())
    }

    /// Strokes of every historical identity shape: positive, negative (blob seed 0), zero, `u64::MAX`,
    /// non-numeric, agent layer, scale != 1, negative z.
    #[rustfmt::skip]
    pub(super) fn strokes(&mut self) -> Result<(), String> {
        const NEG: &str = "-2520638596094309648";
        let neg_seed = NEG.parse::<i64>().map_or(0, i64::cast_unsigned);
        let rows: [StrokeRow; 10] = [
            ("pg_a", "123", "ink", 65_536, -16_777_216, 1.0, 123, 12, Some(123)),
            ("pg_a", NEG, "ink", 131_072, -65_536, 2.0, 0, 20, Some(neg_seed)),
            ("pg_a", "0", "ink", 196_608, -16_776_961, 1.0, 0, 5, Some(0)),
            ("pg_a", "18446744073709551615", "ink", 262_144, -16_711_936, 1.0, u64::MAX, 8, Some(u64::MAX)),
            ("pg_a", "legacy-abc", "ink", 327_680, -1, 1.0, 0, 6, None),
            ("pg_a", "9000000001", "agent", 393_216, -256, 1.0, 9_000_000_001, 9, Some(9_000_000_001)),
            ("pg_b", "55", "ink", -65_536, -16_777_216, 1.0, 55, 4, Some(55)),
            ("pg_b", "56", "ink", 65_536, -16_777_216, 1.5, 56, 30, Some(56)),
            ("pg_in_trash", "77", "ink", 65_536, -16_777_216, 1.0, 77, 3, Some(77)),
            ("pg_gone", "88", "ink", 65_536, -16_777_216, 1.0, 88, 3, Some(88)),
        ];
        for (page, id, layer, z, argb, scale, blob_seed, samples, seed) in rows {
            self.stroke(&StrokeSpec { page, id, layer, z, argb, scale, blob_seed, samples, seed })?;
        }
        Ok(())
    }

    /// Pictures: one back-inserted (negative z) with a label, one on the agent layer.
    pub(super) fn images(&mut self) -> Result<(), String> {
        let json = |id: &str, layer: &str, label: Option<&str>| {
            let label = label.map_or_else(String::new, |l| format!(",\"label\":\"{l}\""));
            format!(
                "{{\"id\":\"{id}\",\"asset\":\"{}\",\"x\":10.0,\"y\":20.0,\"w\":300.0,\"h\":200.0,\"layer\":\"{layer}\"{label}}}",
                "ab".repeat(32)
            )
            .into_bytes()
        };
        let label = Some("Header component");
        let first = json("img_1", "ink", label);
        let row = ObjectRow {
            page: "pg_a",
            id: "img_1",
            kind: "image",
            layer: "ink",
            z: -131_072,
            codec: "image_v1",
            payload: &first,
        };
        self.object(&row)?;
        self.expect.images.push(ImageExpect {
            page: "pg_a".into(),
            row_id: "img_1".into(),
            z: -131_072,
            label: label.map(str::to_owned),
        });
        self.mark("pg_a", true);
        let second = json("img_2", "agent", None);
        let row = ObjectRow {
            page: "pg_b",
            id: "img_2",
            kind: "image",
            layer: "agent",
            z: 0,
            codec: "image_v1",
            payload: &second,
        };
        self.object(&row)?;
        self.expect.images.push(ImageExpect {
            page: "pg_b".into(),
            row_id: "img_2".into(),
            z: 0,
            label: None,
        });
        self.mark("pg_b", true);
        Ok(())
    }

    /// Rows the migration cannot decode: truncated blob, unknown codec, non-finite sample, zero samples,
    /// garbage picture JSON. Each must land in `legacy_unmigrated`.
    pub(super) fn quarantined(&mut self) -> Result<(), String> {
        let (samples, times) = wave(6, 0.0);
        let mut truncated = stroke_blob(3, -1, 1.0, 1, &samples, &times);
        truncated.truncate(truncated.len() - 7);
        let mut nan = samples.clone();
        nan[7] = f32::NAN;
        let nan_blob = stroke_blob(3, -1, 1.0, 2, &nan, &times);
        let empty = stroke_blob(3, -1, 1.0, 3, &[], &[]);
        let bad: [BadRow<'_>; 5] = [
            (
                "pg_a",
                "bad_truncated",
                "stroke",
                458_752,
                "stroke_v1",
                &truncated,
            ),
            (
                "pg_a",
                "bad_codec",
                "stroke",
                524_288,
                "stroke_v2",
                b"future",
            ),
            ("pg_a", "bad_nan", "stroke", 589_824, "stroke_v1", &nan_blob),
            ("pg_a", "bad_empty", "stroke", 655_360, "stroke_v1", &empty),
            ("pg_b", "bad_image", "image", 0, "image_v1", b"{not json"),
        ];
        for (page, id, kind, z, codec, payload) in bad {
            self.object(&ObjectRow {
                page,
                id,
                kind,
                layer: "ink",
                z,
                codec,
                payload,
            })?;
            self.mark(page, false);
        }
        Ok(())
    }
}

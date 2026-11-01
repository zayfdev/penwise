package dev.penwise.overview

import dev.penwise.frames.FrameSpec

internal fun frame(
    id: String,
    x: Float,
    y: Float = 0f,
    route: String = "/",
    preset: String = "desktop",
    label: String = "",
    group: String? = null,
    chosen: Boolean = false,
) = FrameSpec(id, "p", route, preset, "http://x/", x, y, 1440, 900, label = label, group = group, chosen = chosen)

/** Real-format page ids: 32 hex characters, and the 26-character ULIDs the server names the same pages by. */
internal const val SKETCH_1 = "01563e3ab5d3d6764c61efb99302bd5b"
internal const val SKETCH_1_ULID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
internal const val SKETCH_2 = "0102030405060a0b0c0d0e0f10111213"
internal const val SKETCH_2_ULID = "01081G8186185GR38E1W8124GK"
internal const val GONE_PAGE = "ffffffffffffffffffffffffffffffff"

internal fun picture(name: String) = HistoryPicture("item-$name", "asset-$name", 1440, 7574)

internal fun generation(
    round: Int,
    vararg variants: Pair<String, Boolean>,
) = Generation(
    round,
    1_000L * round,
    picture("$round-og"),
    variants.map { (label, picked) -> HistoryVariant(label, picture("$round-$label"), picked) },
)

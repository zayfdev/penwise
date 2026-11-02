package dev.penwise.tools

import org.junit.Assert.assertEquals
import org.junit.Test

/** A 200 px wheel with a 22 px ring and a 100 px square: ring radius 89, square from 50 to 150. */
class WheelGeometryTest {
    private val wheel = WheelGeometry(side = 200f, ringWidth = 22f, squareSide = 100f)

    @Test
    fun touches_land_in_the_ring_the_square_or_outside() {
        assertEquals(WheelZone.RING, wheel.zoneAt(190f, 100f))
        assertEquals(WheelZone.SQUARE, wheel.zoneAt(100f, 100f))
        assertEquals(WheelZone.SQUARE, wheel.zoneAt(149f, 51f))
        assertEquals(WheelZone.NONE, wheel.zoneAt(3f, 3f))
        assertEquals(WheelZone.NONE, wheel.zoneAt(100f, 160f))
    }

    @Test
    fun the_hue_follows_the_angle_clockwise_from_the_right() {
        assertEquals(0f, wheel.hueAt(190f, 100f), 0.01f)
        assertEquals(90f, wheel.hueAt(100f, 190f), 0.01f)
        assertEquals(180f, wheel.hueAt(10f, 100f), 0.01f)
        assertEquals(270f, wheel.hueAt(100f, 10f), 0.01f)
    }

    @Test
    fun the_square_maps_left_to_right_to_saturation_and_bottom_to_top_to_value() {
        assertEquals(0f, wheel.saturationAt(50f), 0f)
        assertEquals(1f, wheel.saturationAt(150f), 0f)
        assertEquals(0.5f, wheel.saturationAt(100f), 0f)
        assertEquals(1f, wheel.valueAt(50f), 0f)
        assertEquals(0f, wheel.valueAt(150f), 0f)
    }

    @Test
    fun a_drag_past_the_square_is_held_at_its_edge() {
        assertEquals(0f, wheel.saturationAt(-300f), 0f)
        assertEquals(1f, wheel.saturationAt(900f), 0f)
        assertEquals(0f, wheel.valueAt(900f), 0f)
    }

    @Test
    fun thumbs_sit_where_the_touch_would_put_them() {
        val ring = wheel.ringPoint(90f)
        assertEquals(100f, ring.x, 0.01f)
        assertEquals(189f, ring.y, 0.01f)
        val square = wheel.squarePoint(saturation = 1f, value = 1f)
        assertEquals(150f, square.x, 0.01f)
        assertEquals(50f, square.y, 0.01f)
    }
}

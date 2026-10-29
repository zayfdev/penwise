package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The core's frame list and the server's project list, as their JSON arrives (C1, C2). */
class FrameSceneTest {
    @Test
    fun frames_carry_their_project_route_preset_and_page_rect() {
        val json =
            """[{"id":"00112233445566778899aabbccddeeff","project":"portfolio","route":"/work","preset":"phone",""" +
                """"cssW":390,"cssH":844,"rect":[160,-40,390,844]}]"""
        assertEquals(
            listOf(
                SceneFrame(
                    id = "00112233445566778899aabbccddeeff",
                    project = "portfolio",
                    route = "/work",
                    preset = "phone",
                    cssWidth = 390,
                    cssHeight = 844,
                    x = 160f,
                    y = -40f,
                ),
            ),
            SceneFrames.parse(json),
        )
    }

    @Test
    fun a_frame_without_a_usable_id_is_skipped() {
        val json =
            """[{"project":"portfolio","rect":[0,0,390,844]},""" +
                """{"id":"00112233445566778899aabbccddeeff","project":"p","rect":[1,2,390,844]}]"""
        assertEquals(1, SceneFrames.parse(json).size)
    }

    @Test
    fun a_malformed_list_is_empty() {
        assertTrue(SceneFrames.parse("not json").isEmpty())
        assertTrue(SceneFrames.parse("").isEmpty())
        assertTrue(SceneFrames.parse("{}").isEmpty())
        assertTrue(SceneFrames.parse("[]").isEmpty())
    }

    @Test
    fun projects_carry_the_port_a_frame_is_loaded_from() {
        val json =
            """[{"id":"prt_1","name":"Portfolio","kind":"static","port":17421,""" +
                """"root":"/home/user/projects/site","static":"site","notebook":"nb_1"}]"""
        val project =
            DesignProject(
                id = "prt_1",
                name = "Portfolio",
                root = "/home/user/projects/site",
                kind = "static",
                port = 17421,
            )
        assertEquals(listOf(project), ProjectList.parse(json))
    }

    @Test
    fun a_project_without_a_port_is_skipped() {
        assertTrue(ProjectList.parse("""[{"id":"prt_1","name":"Portfolio"}]""").isEmpty())
        assertTrue(ProjectList.parse("nonsense").isEmpty())
    }
}

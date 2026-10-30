package dev.penwise

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.View
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalView
import dev.penwise.ink.Clock
import dev.penwise.ink.PEN_AWAKE_MS
import dev.penwise.ink.PenAwake

/**
 * The rule that keeps the screen awake while a page is open: awake while the pen is in use and for [PEN_AWAKE_MS]
 * after its last input, then the system screen timeout applies. Opening the page does not wake it and finger input
 * does not count; only the pen does. Hand the result to the canvas as its pen-input listener. Cleared on leaving.
 */
@Composable
fun rememberPenAwake(): PenAwake {
    val view = LocalView.current
    val awake = remember(view) { newPenAwake(view) }
    DisposableEffect(awake) { onDispose { awake.stop() } }
    return awake
}

/** The rule on the main looper: one reused Runnable, so a pen sample never posts anything. */
private fun newPenAwake(view: View): PenAwake {
    val handler = Handler(Looper.getMainLooper())
    lateinit var awake: PenAwake
    val check = Runnable { awake.check() }
    awake =
        PenAwake(
            clock = Clock { SystemClock.uptimeMillis() },
            schedule = { handler.postDelayed(check, it) },
            cancel = { handler.removeCallbacks(check) },
            onChange = { view.keepScreenOn = it },
        )
    return awake
}

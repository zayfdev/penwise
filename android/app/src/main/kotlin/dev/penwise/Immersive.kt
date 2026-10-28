package dev.penwise

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.view.ViewTreeObserver
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/**
 * While composed (a page is open): hide the status and navigation bars so an edge swipe only peeks them
 * (transient bars) instead of acting as Back/Home, and keep the screen awake. Restored on leaving.
 * Bars are hidden again whenever the window regains focus (dialogs and the transient peek drop the flag).
 */
@Composable
fun ImmersiveWhileOpen() {
    val view = LocalView.current
    DisposableEffect(view) {
        val window = view.context.findActivity()?.window
        if (window == null) {
            onDispose {}
        } else {
            val controller = WindowCompat.getInsetsController(window, view)

            fun hide() {
                controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                controller.hide(WindowInsetsCompat.Type.systemBars())
            }
            val onFocus = ViewTreeObserver.OnWindowFocusChangeListener { focused -> if (focused) hide() }
            hide()
            view.keepScreenOn = true
            view.viewTreeObserver.addOnWindowFocusChangeListener(onFocus)
            onDispose {
                view.viewTreeObserver.removeOnWindowFocusChangeListener(onFocus)
                view.keepScreenOn = false
                controller.show(WindowInsetsCompat.Type.systemBars())
            }
        }
    }
}

private tailrec fun Context.findActivity(): Activity? =
    when (this) {
        is Activity -> this
        is ContextWrapper -> baseContext.findActivity()
        else -> null
    }

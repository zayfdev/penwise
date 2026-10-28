package dev.penwise.ink

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import dev.penwise.R

/**
 * Keeps the app's sync socket alive while the screen is off or the app is in the background.
 *
 * The service owns no connection: the sync engine lives with the view model. What this does is give the
 * process foreground priority and hold a Wi-Fi lock, which is what Android otherwise revokes after the screen
 * sleeps — the socket then dies with "software caused connection abort" and every reconnect re-announces the page.
 *
 * The Wi-Fi lock is held only while the socket is online ([EXTRA_ONLINE]): offline there is nothing to keep alive.
 *
 * Deliberately no CPU wake lock: the socket pings every 20 s, and each ping wakes the CPU briefly through the
 * Wi-Fi chip. A permanent partial lock would keep the processor busy for nothing.
 */
class SyncService : Service() {
    private var wifiLock: WifiManager.WifiLock? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
    }

    override fun onStartCommand(
        intent: Intent?,
        flags: Int,
        startId: Int,
    ): Int {
        val notification = buildNotification(intent?.getStringExtra(EXTRA_STATUS).orEmpty())
        try {
            // minSdk is 34, so the typed startForeground is always available.
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } catch (error: IllegalStateException) {
            // ForegroundServiceStartNotAllowedException: the app is in the background and may not start one.
            Log.w(TAG, "foreground sync not allowed: ${error.message}")
            stopSelf()
            return START_NOT_STICKY
        }
        if (intent?.getBooleanExtra(EXTRA_ONLINE, false) == true) acquireLocks() else releaseLocks()
        Log.i(TAG, "foreground sync started")
        // Not sticky: if the system kills us, the app restarting is enough to bring sync back.
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    /** Android 15 caps a dataSync service at about 6 h a day; past that it must stop itself or the app is killed. */
    override fun onTimeout(
        startId: Int,
        fgsType: Int,
    ) {
        Log.w(TAG, "foreground sync timed out; stopping")
        releaseLocks()
        stopSelf()
    }

    override fun onDestroy() {
        releaseLocks()
        Log.i(TAG, "foreground sync stopped")
        super.onDestroy()
    }

    private fun acquireLocks() {
        if (wifiLock == null) {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            wifiLock =
                wifi
                    ?.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "$TAG:wifi")
                    ?.apply {
                        setReferenceCounted(false)
                        acquire()
                    }
        }
    }

    private fun releaseLocks() {
        wifiLock?.takeIf { it.isHeld }?.release()
        wifiLock = null
    }

    private fun createChannel() {
        val manager = getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, getString(R.string.sync_channel), NotificationManager.IMPORTANCE_LOW)
                .apply { description = getString(R.string.sync_channel_description) },
        )
    }

    private fun buildNotification(status: String): Notification {
        val text = status.ifBlank { getString(R.string.sync_running) }
        return NotificationCompat
            .Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_sync)
            .setContentTitle(getString(R.string.sync_title))
            .setContentText(text)
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()
    }

    companion object {
        private const val TAG = "PenwiseSyncService"
        private const val CHANNEL_ID = "sync"
        private const val NOTIFICATION_ID = 1

        /** Extra carrying the status line the notification shows. */
        const val EXTRA_STATUS = "status"

        /** Extra that is true while the socket is online (the Wi-Fi lock is held only then). */
        const val EXTRA_ONLINE = "online"

        /**
         * Starts (or updates) the foreground service; safe to call repeatedly. Android refuses a foreground start
         * from the background (`ForegroundServiceStartNotAllowedException`); that must never reach the caller, which
         * is often the middle of handling a server frame.
         */
        fun start(
            context: Context,
            status: String = "",
            online: Boolean = false,
        ) {
            val intent =
                Intent(context, SyncService::class.java)
                    .putExtra(EXTRA_STATUS, status)
                    .putExtra(EXTRA_ONLINE, online)
            try {
                ContextCompat.startForegroundService(context, intent)
            } catch (error: IllegalStateException) {
                Log.w(TAG, "cannot start the foreground sync service now: ${error.message}")
            }
        }

        /** Stops it when the app goes away. */
        fun stop(context: Context) {
            context.stopService(Intent(context, SyncService::class.java))
        }
    }
}

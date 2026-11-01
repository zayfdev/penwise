package dev.penwise.sync

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities

/** [NetworkSource] over the system's default-network callback: fires when the default network becomes validated. */
class AndroidNetworkSource(
    context: Context,
) : NetworkSource {
    private val manager = context.applicationContext.getSystemService(ConnectivityManager::class.java)

    override fun active(): Any? = manager?.activeNetwork

    override fun watch(onValidated: (network: Any) -> Unit): AutoCloseable {
        val connectivity = manager ?: return AutoCloseable {}
        var validated: Network? = null
        val callback =
            object : ConnectivityManager.NetworkCallback() {
                override fun onCapabilitiesChanged(
                    network: Network,
                    capabilities: NetworkCapabilities,
                ) {
                    if (!capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)) {
                        // Validation lost: when it returns, that is a fresh "usable" event.
                        if (validated == network) validated = null
                        return
                    }
                    if (validated == network) return
                    validated = network
                    onValidated(network)
                }

                override fun onLost(network: Network) {
                    if (validated == network) validated = null
                }
            }
        connectivity.registerDefaultNetworkCallback(callback)
        return AutoCloseable { runCatching { connectivity.unregisterNetworkCallback(callback) } }
    }
}

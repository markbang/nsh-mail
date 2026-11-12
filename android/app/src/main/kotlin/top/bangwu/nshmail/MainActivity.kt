package top.bangwu.nshmail

import android.app.Activity
import android.content.Intent
import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.View
import android.view.WindowInsetsController
import com.longbridge.nsh.NshView

/** The app's bundle in an NshView, as nsh's own AppActivity runs it, plus the FCM token as the host event `push.token`. */
class MainActivity : Activity() {
    private lateinit var nsh: NshView
    private val sendToken: (String) -> Unit = { token -> nsh.send("push.token", mapOf("kind" to "fcm", "token" to token)) }

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        edgeToEdge()
        nsh = NshView(this)
        nsh.setErrorListener { message -> Log.e(TAG, message) }
        setContentView(nsh)
        nsh.handleNotificationIntent(intent)
        // A recreated activity's intent was handed in already.
        if (state == null) nsh.handleUrlSchemeIntent(intent)
        nsh.loadAssetBundle("nsh")
        PushToken.listen(this, sendToken)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        nsh.handleNotificationIntent(intent)
        nsh.handleUrlSchemeIntent(intent)
    }

    override fun onConfigurationChanged(config: Configuration) {
        super.onConfigurationChanged(config)
        systemBars()
    }

    /** Android 8 hands back presses to the activity first. */
    @Deprecated("Android 12 and older")
    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        if (!nsh.handleBack()) super.onBackPressed()
    }

    override fun onDestroy() {
        PushToken.stop(sendToken)
        nsh.destroy()
        super.onDestroy()
    }

    /** Draws behind the system bars, which the app gets as its safe area. */
    @Suppress("DEPRECATION")
    private fun edgeToEdge() {
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false)
        } else {
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        }
        window.statusBarColor = 0
        window.navigationBarColor = 0
        systemBars()
    }

    /** Dark bar icons in light mode, light ones in dark mode. */
    @Suppress("DEPRECATION")
    private fun systemBars() {
        val dark = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
        val decor = window.decorView
        if (Build.VERSION.SDK_INT >= 30) {
            val light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            window.insetsController?.setSystemBarsAppearance(if (dark) 0 else light, light)
        } else {
            val light = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
            decor.systemUiVisibility = if (dark) decor.systemUiVisibility and light.inv() else decor.systemUiVisibility or light
        }
    }

    private companion object {
        const val TAG = "nsh-mail"
    }
}

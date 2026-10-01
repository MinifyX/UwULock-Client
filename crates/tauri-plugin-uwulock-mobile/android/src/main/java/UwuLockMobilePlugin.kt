package app.uwulock.mobile

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.wifi.WifiEnterpriseConfig
import android.net.wifi.WifiManager
import android.net.wifi.WifiNetworkSuggestion
import android.os.Build
import android.provider.MediaStore
import android.provider.Settings
import android.view.View
import android.webkit.MimeTypeMap
import android.os.PersistableBundle
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import androidx.activity.result.ActivityResult
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import java.security.cert.X509Certificate
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class NameArgs {
    lateinit var name: String
}

@InvokeArg
class PromptArgs {
    lateinit var name: String
    lateinit var title: String
    var subtitle: String = ""
    lateinit var cancel: String
}

@InvokeArg
class AppearanceArgs {
    var dark: Boolean = true
    lateinit var background: String
}

@InvokeArg
class SaveArgs {
    lateinit var path: String
    lateinit var name: String
}

@InvokeArg
class CopyArgs {
    lateinit var text: String
    var expiresInSeconds: Long? = null
}

/** A Wi-Fi network, as the app's `wifi.rs` hands it over (already checked there). */
@InvokeArg
class WifiArgs {
    lateinit var ssid: String
    /** `open`, `wpa2`, `wpa3`, `wpa2-enterprise` or `wpa3-enterprise`. */
    lateinit var security: String
    var password: String? = null
    var hidden: Boolean = false
    /** Enterprise: `PEAP`, `TTLS` or `PWD`. */
    var eap: String? = null
    /** Enterprise: `MSCHAPV2`, `PAP`, `GTC` or `NONE`. */
    var phase2: String? = null
    var identity: String? = null
    var anonymousIdentity: String? = null
    /** PEAP and TTLS: the RADIUS server's domain, checked against the system's CAs. */
    var domain: String? = null
}

/**
 * UwULock's own Android code, called from Rust (tauri-plugin-uwulock-mobile).
 *
 * Unlocking with a fingerprint or face: 32 random bytes per account, encrypted
 * under an AES key in the Android Keystore that only works right after
 * BiometricPrompt confirmed a strong biometric (class 3), and that Android
 * deletes when a new finger or face is enrolled. Only the encrypted bytes are
 * kept, in UwULock's private preferences (no backup, see the app's manifest).
 * Rust stretches the bytes into the key that seals the account's user key.
 *
 * Copying: the clip is marked sensitive, so Android 13+ shows no preview and
 * keyboards keep it out of their clipboard history.
 *
 * Joining a Wi-Fi network: Android 11+ shows its own sheet to add the network
 * (Settings.ACTION_WIFI_ADD_NETWORKS) and the person confirms there; Android 10
 * gets it as a network suggestion. Needs CHANGE_WIFI_STATE (a normal permission,
 * granted at install) for the suggestion, nothing else — no location.
 *
 * New phone features get a @Command here and a method in src/lib.rs — see
 * docs/mobile.md.
 */
@TauriPlugin
class UwuLockMobilePlugin(private val activity: Activity) : Plugin(activity) {
    companion object {
        private const val TAG = "UwULock"
        private const val PREFS = "uwulock.unlock"
        private const val KEY_PREFIX = "uwulock-unlock-"
        private const val IV_LENGTH = 12
        private const val CLIP_LABEL = "UwULock"
    }

    /** The text of UwULock's last copy, so a later clear leaves anything copied since alone. */
    @Volatile
    private var lastCopy: String? = null

    // ── Unlocking with a biometric ────────────────────────────

    @Command
    fun unlockStatus(invoke: Invoke) {
        val result = JSObject()
        val answer = BiometricManager.from(activity).canAuthenticate(BIOMETRIC_STRONG)
        result.put("available", answer == BiometricManager.BIOMETRIC_SUCCESS)
        result.put("kind", kind())
        when (answer) {
            BiometricManager.BIOMETRIC_SUCCESS -> {}
            BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> result.put("reason", "none-enrolled")
            BiometricManager.BIOMETRIC_ERROR_NO_HARDWARE -> result.put("reason", "no-hardware")
            BiometricManager.BIOMETRIC_ERROR_SECURITY_UPDATE_REQUIRED -> result.put("reason", "security-update")
            else -> result.put("reason", "unavailable")
        }
        invoke.resolve(result)
    }

    private fun kind(): String {
        val pm = activity.packageManager
        val kinds = listOf(
            "android.hardware.fingerprint" to "fingerprint",
            "android.hardware.biometrics.face" to "face",
            "android.hardware.biometrics.iris" to "iris",
        ).filter { pm.hasSystemFeature(it.first) }
        return if (kinds.size == 1) kinds[0].second else "biometric"
    }

    @Command
    fun unlockCreate(invoke: Invoke) {
        val args = invoke.parseArgs(PromptArgs::class.java)
        val cipher = try {
            deleteKey(args.name)
            Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, newKey(args.name)) }
        } catch (error: Exception) {
            Log.w(TAG, "couldn't make the unlock key", error)
            invoke.reject("The phone couldn't make a key for unlocking: ${error.message}", "failed")
            return
        }
        prompt(invoke, args, cipher) { done ->
            val secret = ByteArray(32).also { SecureRandom().nextBytes(it) }
            try {
                val sealed = done.iv + done.doFinal(secret)
                val saved = prefs().edit()
                    .putString(args.name, Base64.encodeToString(sealed, Base64.NO_WRAP))
                    .commit()
                if (!saved) throw IllegalStateException("not saved")
                invoke.resolve(secretObject(secret))
            } finally {
                secret.fill(0)
            }
        }
    }

    @Command
    fun unlockOpen(invoke: Invoke) {
        val args = invoke.parseArgs(PromptArgs::class.java)
        val stored = prefs().getString(args.name, null)
        val key = existingKey(args.name)
        if (stored == null || key == null) {
            invoke.reject("Nothing is kept for unlocking on this phone.", "missing")
            return
        }
        val sealed = Base64.decode(stored, Base64.NO_WRAP)
        val cipher = try {
            Cipher.getInstance("AES/GCM/NoPadding").apply {
                init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, sealed, 0, IV_LENGTH))
            }
        } catch (error: KeyPermanentlyInvalidatedException) {
            // A new finger or face was enrolled: the key is gone for good.
            forget(args.name)
            invoke.reject("A fingerprint or face was added on this phone since.", "invalidated")
            return
        } catch (error: Exception) {
            Log.w(TAG, "couldn't open the unlock key", error)
            invoke.reject("The phone couldn't use its key for unlocking: ${error.message}", "failed")
            return
        }
        prompt(invoke, args, cipher) { done ->
            val secret = done.doFinal(sealed, IV_LENGTH, sealed.size - IV_LENGTH)
            try {
                invoke.resolve(secretObject(secret))
            } finally {
                secret.fill(0)
            }
        }
    }

    @Command
    fun unlockDelete(invoke: Invoke) {
        forget(invoke.parseArgs(NameArgs::class.java).name)
        invoke.resolve()
    }

    /** BiometricPrompt, tied to the cipher: the key only works for this one operation after it. */
    private fun prompt(invoke: Invoke, args: PromptArgs, cipher: Cipher, then: (Cipher) -> Unit) {
        val host = activity as? FragmentActivity
        if (host == null) {
            invoke.reject("No window to ask in.", "unavailable")
            return
        }
        host.runOnUiThread {
            val callback = object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val unlocked = result.cryptoObject?.cipher
                    if (unlocked == null) {
                        invoke.reject("The phone gave no key back.", "failed")
                        return
                    }
                    try {
                        then(unlocked)
                    } catch (error: Exception) {
                        Log.w(TAG, "unlock key operation failed", error)
                        invoke.reject("The phone's key didn't work: ${error.message}", "failed")
                    }
                }

                override fun onAuthenticationError(code: Int, message: CharSequence) {
                    val kind = when (code) {
                        BiometricPrompt.ERROR_USER_CANCELED,
                        BiometricPrompt.ERROR_NEGATIVE_BUTTON,
                        BiometricPrompt.ERROR_CANCELED -> "cancelled"
                        BiometricPrompt.ERROR_LOCKOUT,
                        BiometricPrompt.ERROR_LOCKOUT_PERMANENT -> "lockout"
                        BiometricPrompt.ERROR_NO_BIOMETRICS,
                        BiometricPrompt.ERROR_HW_NOT_PRESENT,
                        BiometricPrompt.ERROR_HW_UNAVAILABLE -> "unavailable"
                        else -> "failed"
                    }
                    invoke.reject(message.toString(), kind)
                }
                // A finger that doesn't match: the prompt stays and says so itself.
            }
            val info = BiometricPrompt.PromptInfo.Builder()
                .setTitle(args.title)
                .apply { if (args.subtitle.isNotEmpty()) setSubtitle(args.subtitle) }
                .setNegativeButtonText(args.cancel)
                .setAllowedAuthenticators(BIOMETRIC_STRONG)
                .setConfirmationRequired(false)
                .build()
            try {
                BiometricPrompt(host, ContextCompat.getMainExecutor(host), callback)
                    .authenticate(info, BiometricPrompt.CryptoObject(cipher))
            } catch (error: Exception) {
                Log.w(TAG, "couldn't show the biometric prompt", error)
                invoke.reject("The phone couldn't ask for a fingerprint or face: ${error.message}", "failed")
            }
        }
    }

    private fun secretObject(secret: ByteArray): JSObject {
        val result = JSObject()
        result.put("secret", Base64.encodeToString(secret, Base64.NO_WRAP))
        return result
    }

    private fun prefs() = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    private fun keyStore() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    private fun existingKey(name: String): SecretKey? = keyStore().getKey(KEY_PREFIX + name, null) as? SecretKey

    private fun newKey(name: String): SecretKey {
        val spec = KeyGenParameterSpec.Builder(
            KEY_PREFIX + name,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUserAuthenticationRequired(true)
            .setInvalidatedByBiometricEnrollment(true)
            .apply {
                if (Build.VERSION.SDK_INT >= 30) {
                    // Every use needs its own biometric check, never a device PIN.
                    setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
                }
            }
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
            .apply { init(spec) }
            .generateKey()
    }

    private fun deleteKey(name: String) {
        try {
            keyStore().deleteEntry(KEY_PREFIX + name)
        } catch (error: Exception) {
            Log.w(TAG, "couldn't delete an unlock key", error)
        }
    }

    private fun forget(name: String) {
        deleteKey(name)
        prefs().edit().remove(name).commit()
    }

    // ── Copying ───────────────────────────────────────────────

    @Command
    fun copySecret(invoke: Invoke) {
        val args = invoke.parseArgs(CopyArgs::class.java)
        activity.runOnUiThread {
            try {
                val clip = ClipData.newPlainText(CLIP_LABEL, args.text)
                val extras = PersistableBundle()
                // Android 13+: no preview in the copy confirmation, kept out of keyboard histories.
                extras.putBoolean(
                    if (Build.VERSION.SDK_INT >= 33) ClipDescription.EXTRA_IS_SENSITIVE else "android.content.extra.IS_SENSITIVE",
                    true,
                )
                clip.description.extras = extras
                clipboard().setPrimaryClip(clip)
                lastCopy = args.text
                invoke.resolve()
            } catch (error: Exception) {
                invoke.reject("Couldn't copy: ${error.message}", "failed")
            }
        }
    }

    @Command
    fun clearClipboard(invoke: Invoke) {
        activity.runOnUiThread {
            val ours = lastCopy
            lastCopy = null
            try {
                val manager = clipboard()
                if (ours != null && manager.hasPrimaryClip()) {
                    // Android only lets an app read the clipboard while it is in front. In the
                    // background, a clip with UwULock's label counts as UwULock's.
                    val clip = manager.primaryClip
                    val mine = if (clip != null && clip.itemCount > 0) {
                        clip.getItemAt(0).text?.toString() == ours
                    } else {
                        manager.primaryClipDescription?.label == CLIP_LABEL
                    }
                    if (mine) manager.clearPrimaryClip()
                }
            } catch (error: Exception) {
                Log.w(TAG, "couldn't clear the clipboard", error)
            }
            invoke.resolve()
        }
    }

    private fun clipboard() = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager

    // ── The window ────────────────────────────────────────────

    /** The bars around the page take its background, with icons that stay readable on it. */
    @Command
    fun setAppearance(invoke: Invoke) {
        val args = invoke.parseArgs(AppearanceArgs::class.java)
        activity.runOnUiThread {
            val color = try {
                Color.parseColor(args.background)
            } catch (error: IllegalArgumentException) {
                if (args.dark) Color.BLACK else Color.WHITE
            }
            activity.window.decorView.setBackgroundColor(color)
            activity.findViewById<View>(android.R.id.content)?.setBackgroundColor(color)
            WindowCompat.getInsetsController(activity.window, activity.window.decorView).apply {
                isAppearanceLightStatusBars = !args.dark
                isAppearanceLightNavigationBars = !args.dark
            }
            invoke.resolve()
        }
    }

    // ── Joining a Wi-Fi network ───────────────────────────────

    @Command
    fun connectWifi(invoke: Invoke) {
        val args = invoke.parseArgs(WifiArgs::class.java)
        if (Build.VERSION.SDK_INT < 29) {
            invoke.reject("This Android can't take networks from apps.", "unsupported")
            return
        }
        val suggestion = try {
            suggestion(args)
        } catch (error: IllegalArgumentException) {
            // The builder checks the values itself (a password Android doesn't take, …).
            invoke.reject("Android didn't take the network: ${error.message}", "invalid")
            return
        } catch (error: Exception) {
            Log.w(TAG, "couldn't describe the network", error)
            invoke.reject("Couldn't describe the network: ${error.message}", "failed")
            return
        }
        if (Build.VERSION.SDK_INT >= 30) {
            // The intent carries the password: only the system's own sheet may get it, never an
            // app that registered for the same action (which would show up in a chooser).
            val sheet = systemActivity(Intent(Settings.ACTION_WIFI_ADD_NETWORKS))
            if (sheet == null) {
                suggest(invoke, suggestion)
                return
            }
            val intent = Intent(Settings.ACTION_WIFI_ADD_NETWORKS)
                .setComponent(sheet)
                .putParcelableArrayListExtra(Settings.EXTRA_WIFI_NETWORK_LIST, arrayListOf(suggestion))
            activity.runOnUiThread {
                try {
                    startActivityForResult(invoke, intent, "wifiAdded")
                } catch (error: ActivityNotFoundException) {
                    // A phone without the sheet: suggest the network instead.
                    suggest(invoke, suggestion)
                } catch (error: Exception) {
                    Log.w(TAG, "couldn't open the add-network sheet", error)
                    invoke.reject("Couldn't open Android's Wi-Fi sheet: ${error.message}", "failed")
                }
            }
        } else {
            suggest(invoke, suggestion)
        }
    }

    // Tauri finds the callback by its name; kept public like Tauri's own plugins do.
    @ActivityCallback
    fun wifiAdded(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            invoke.resolve(outcome("declined"))
            return
        }
        val codes = result.data?.getIntegerArrayListExtra(Settings.EXTRA_WIFI_NETWORK_RESULT_LIST)
        when (codes?.firstOrNull()) {
            Settings.ADD_WIFI_RESULT_ALREADY_EXISTS -> invoke.resolve(outcome("already-saved"))
            Settings.ADD_WIFI_RESULT_ADD_OR_UPDATE_FAILED ->
                invoke.reject("Android couldn't save the network.", "failed")
            // Some phones answer OK without the list: the person confirmed.
            else -> invoke.resolve(outcome("saved"))
        }
    }

    /** The activity of a system app (preinstalled, not updatable by others) that handles `intent`. */
    private fun systemActivity(intent: Intent): ComponentName? {
        val pm = activity.packageManager
        val found = if (Build.VERSION.SDK_INT >= 33) {
            pm.queryIntentActivities(intent, PackageManager.ResolveInfoFlags.of(PackageManager.MATCH_SYSTEM_ONLY.toLong()))
        } else {
            @Suppress("DEPRECATION")
            pm.queryIntentActivities(intent, PackageManager.MATCH_SYSTEM_ONLY)
        }
        val info = found
            .mapNotNull { it.activityInfo }
            .firstOrNull { (it.applicationInfo.flags and ApplicationInfo.FLAG_SYSTEM) != 0 }
            ?: return null
        return ComponentName(info.packageName, info.name)
    }

    /** Android 10: a suggestion the phone joins by itself once the person allowed UwULock's. */
    private fun suggest(invoke: Invoke, suggestion: WifiNetworkSuggestion) {
        val wifi = activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        var status = wifi.addNetworkSuggestions(listOf(suggestion))
        if (status == WifiManager.STATUS_NETWORK_SUGGESTIONS_ERROR_ADD_DUPLICATE) {
            // Suggested before, maybe with an old password: replace it.
            wifi.removeNetworkSuggestions(listOf(suggestion))
            status = wifi.addNetworkSuggestions(listOf(suggestion))
        }
        when (status) {
            WifiManager.STATUS_NETWORK_SUGGESTIONS_SUCCESS -> invoke.resolve(outcome("suggested"))
            // The person once said no to UwULock's suggestions (Settings → Apps → Special app access).
            WifiManager.STATUS_NETWORK_SUGGESTIONS_ERROR_APP_DISALLOWED -> invoke.resolve(outcome("disallowed"))
            else -> invoke.reject("Android didn't take the suggestion (status $status).", "failed")
        }
    }

    /**
     * Android's Wi-Fi settings, for a network UwULock can't hand over (WEP, EAP-TLS, a CA
     * certificate that isn't a domain): the app copied the password before.
     */
    @Command
    fun openWifiSettings(invoke: Invoke) {
        activity.runOnUiThread {
            try {
                activity.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS))
                invoke.resolve()
            } catch (error: Exception) {
                invoke.reject("Couldn't open the Wi-Fi settings: ${error.message}", "failed")
            }
        }
    }

    private fun outcome(name: String) = JSObject().apply { put("outcome", name) }

    private fun suggestion(args: WifiArgs): WifiNetworkSuggestion {
        val builder = WifiNetworkSuggestion.Builder()
            .setSsid(args.ssid)
            .setIsHiddenSsid(args.hidden)
        when (args.security) {
            "open" -> {}
            "wpa2" -> builder.setWpa2Passphrase(args.password ?: "")
            "wpa3" -> builder.setWpa3Passphrase(args.password ?: "")
            "wpa2-enterprise" -> builder.setWpa2EnterpriseConfig(enterprise(args))
            "wpa3-enterprise" -> {
                val config = enterprise(args)
                if (Build.VERSION.SDK_INT >= 31) {
                    builder.setWpa3EnterpriseStandardModeConfig(config)
                } else {
                    @Suppress("DEPRECATION")
                    builder.setWpa3EnterpriseConfig(config)
                }
            }
            else -> throw IllegalArgumentException("unknown security ${args.security}")
        }
        return builder.build()
    }

    private fun enterprise(args: WifiArgs): WifiEnterpriseConfig {
        val config = WifiEnterpriseConfig()
        config.setEapMethod(when (args.eap) {
            "PEAP" -> WifiEnterpriseConfig.Eap.PEAP
            "TTLS" -> WifiEnterpriseConfig.Eap.TTLS
            "PWD" -> WifiEnterpriseConfig.Eap.PWD
            else -> throw IllegalArgumentException("unknown EAP method ${args.eap}")
        })
        config.setPhase2Method(when (args.phase2) {
            "MSCHAPV2" -> WifiEnterpriseConfig.Phase2.MSCHAPV2
            "PAP" -> WifiEnterpriseConfig.Phase2.PAP
            "GTC" -> WifiEnterpriseConfig.Phase2.GTC
            else -> WifiEnterpriseConfig.Phase2.NONE
        })
        config.setIdentity(args.identity ?: "")
        args.anonymousIdentity?.let { config.setAnonymousIdentity(it) }
        config.setPassword(args.password ?: "")
        val domain = args.domain
        if (args.eap != "PWD") {
            // Android insists on checking the RADIUS server: its name against the domain, its
            // certificate against the CAs the phone ships with (what Settings calls "Use system
            // certificates"; apps have no shortcut to that, so they go in one by one).
            require(!domain.isNullOrEmpty()) { "PEAP and TTLS need the server's domain" }
            config.setDomainSuffixMatch(domain)
            config.setCaCertificates(systemCertificates())
        }
        return config
    }

    /** The phone's built-in CA certificates, without any the person added themselves. */
    private fun systemCertificates(): Array<X509Certificate> {
        val store = KeyStore.getInstance("AndroidCAStore").apply { load(null) }
        val certificates = store.aliases().toList()
            .filter { it.startsWith("system:") }
            .mapNotNull { store.getCertificate(it) as? X509Certificate }
        require(certificates.isNotEmpty()) { "no system certificates" }
        return certificates.toTypedArray()
    }

    // ── Files ─────────────────────────────────────────────────

    /**
     * Moves a file UwULock wrote into its own cache into the phone's Downloads folder (MediaStore,
     * no permission needed), and deletes the cached one. Answers with the name it got there.
     */
    @Command
    fun saveToDownloads(invoke: Invoke) {
        val args = invoke.parseArgs(SaveArgs::class.java)
        val source = File(args.path)
        try {
            val extension = args.name.substringAfterLast('.', "").lowercase()
            val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension) ?: "application/octet-stream"
            val values = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, args.name)
                put(MediaStore.Downloads.MIME_TYPE, mime)
                put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val resolver = activity.contentResolver
            val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                ?: throw IllegalStateException("no place in Downloads")
            try {
                resolver.openOutputStream(uri)?.use { out -> source.inputStream().use { it.copyTo(out) } }
                    ?: throw IllegalStateException("Downloads didn't open")
                values.clear()
                values.put(MediaStore.Downloads.IS_PENDING, 0)
                resolver.update(uri, values, null, null)
            } catch (error: Exception) {
                resolver.delete(uri, null, null)
                throw error
            }
            val saved = resolver.query(uri, arrayOf(MediaStore.Downloads.DISPLAY_NAME), null, null, null)?.use {
                if (it.moveToFirst()) it.getString(0) else null
            } ?: args.name
            val result = JSObject()
            result.put("name", saved)
            invoke.resolve(result)
        } catch (error: Exception) {
            Log.w(TAG, "couldn't save into Downloads", error)
            invoke.reject("Couldn't save into Downloads: ${error.message}", "failed")
        } finally {
            source.delete()
        }
    }
}

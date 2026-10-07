package app.uwulock.mobile

import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import androidx.annotation.RequiresApi
import androidx.credentials.provider.CallingAppInfo
import org.json.JSONArray
import org.json.JSONObject
import java.security.MessageDigest

/**
 * Passwords for other apps and browsers (Credential Manager and the autofill service), through
 * the same way into Rust as passkeys (PasskeyBridge, passkeys/android_logins.rs in the app).
 *
 * Kotlin only says who asks; Rust decides which logins that caller gets and hands a password over
 * only for a login of that caller, after the person was verified. Listing never carries passwords.
 */
object LoginBridge {
    private const val TAG = "UwULock"

    class Login(val itemId: String, val name: String, val userName: String?)

    /** A password, after the person was verified. */
    class Filled(val userName: String, val password: String)

    @Volatile
    private var browsers: Map<String, Set<String>>? = null

    private fun allowlist(context: Context): String =
        context.resources.openRawResource(R.raw.privileged_browsers).bufferedReader().use { it.readText() }

    /** The privileged browsers: package → SHA-256 fingerprints (`AA:BB:…`, upper case). */
    private fun browsers(context: Context): Map<String, Set<String>> {
        browsers?.let { return it }
        val out = mutableMapOf<String, MutableSet<String>>()
        try {
            val apps = JSONObject(allowlist(context)).optJSONArray("apps") ?: JSONArray()
            for (i in 0 until apps.length()) {
                val info = apps.optJSONObject(i)?.optJSONObject("info") ?: continue
                val name = info.optString("package_name")
                val signatures = info.optJSONArray("signatures") ?: continue
                for (j in 0 until signatures.length()) {
                    val fingerprint = signatures.optJSONObject(j)?.optString("cert_fingerprint_sha256") ?: continue
                    if (name.isNotEmpty() && fingerprint.isNotEmpty()) {
                        out.getOrPut(name) { mutableSetOf() }.add(fingerprint.uppercase())
                    }
                }
            }
        } catch (error: Exception) {
            Log.w(TAG, "the privileged browser list didn't read: $error")
        }
        return out.also { browsers = it }
    }

    /** SHA-256 of an installed app's signing certificates; empty when Android doesn't show it. */
    private fun certificates(context: Context, packageName: String): List<ByteArray> = try {
        val info = if (Build.VERSION.SDK_INT >= 33) {
            context.packageManager.getPackageInfo(
                packageName, PackageManager.PackageInfoFlags.of(PackageManager.GET_SIGNING_CERTIFICATES.toLong()))
        } else {
            @Suppress("DEPRECATION")
            context.packageManager.getPackageInfo(packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        }
        val digest = MessageDigest.getInstance("SHA-256")
        info.signingInfo?.apkContentsSigners?.map { digest.digest(it.toByteArray()) } ?: emptyList()
    } catch (error: Exception) {
        emptyList()
    }

    private fun fingerprint(hash: ByteArray): String = hash.joinToString(":") { "%02X".format(it) }

    /**
     * Credential Manager: the browser's origin when it is on the privileged list (Android checked
     * it against the browser's certificate), the app by package and certificates otherwise.
     */
    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun caller(context: Context, info: CallingAppInfo): JSONObject {
        val args = JSONObject()
            .put("packageName", info.packageName)
            .put("certHashes", PasskeyBridge.certHashes(info.signingInfo))
        try {
            val origin = info.getOrigin(allowlist(context))
            if (!origin.isNullOrEmpty()) args.put("origin", origin.trimEnd('/'))
        } catch (error: Exception) {
            Log.w(TAG, "no privileged origin for ${info.packageName}: $error")
        }
        return args
    }

    /**
     * The autofill service: a page's domain counts as its address only in a browser on the
     * privileged list, signed with the listed certificate. Any other app's page (a WebView) is
     * only a site to ask whether it shares sign-ins with that app.
     */
    fun autofillCaller(context: Context, fields: FillFields): JSONObject {
        val certs = certificates(context, fields.packageName)
        val args = JSONObject().put("packageName", fields.packageName)
        val domain = fields.webDomain
        val listed = browsers(context)[fields.packageName]
        val browser = listed != null && certs.isNotEmpty() && certs.all { fingerprint(it) in listed }
        if (domain != null && browser) {
            val scheme = fields.webScheme?.takeIf { it == "http" || it == "https" } ?: "https"
            args.put("origin", "$scheme://$domain")
            return args
        }
        val hashes = JSONArray()
        certs.forEach { hashes.put(PasskeyBridge.b64(it)) }
        args.put("certHashes", hashes)
        if (domain != null) args.put("webDomain", domain)
        return args
    }

    /** The logins for the caller, by name. `null` when the vault isn't open. */
    fun logins(args: JSONObject): List<Login>? {
        val answer = PasskeyBridge.call("logins", args)
        if (answer.has("error") || answer.optBoolean("locked", true)) return null
        val list = answer.optJSONArray("logins") ?: JSONArray()
        return (0 until list.length()).mapNotNull { i ->
            val login = list.optJSONObject(i) ?: return@mapNotNull null
            val user = if (login.isNull("userName")) null else login.optString("userName").ifEmpty { null }
            Login(login.optString("itemId"), login.optString("name"), user)
        }
    }

    /** User name and password of the picked login; only call after the person was verified. */
    fun password(args: JSONObject, itemId: String): Filled? {
        val answer = PasskeyBridge.call("password", JSONObject(args.toString()).put("itemId", itemId).put("verified", true))
        if (answer.has("error")) {
            Log.w(TAG, "no password: ${answer.optString("error")}")
            return null
        }
        val password = answer.optString("password")
        if (password.isEmpty()) return null
        return Filled(answer.optString("userName"), password)
    }
}

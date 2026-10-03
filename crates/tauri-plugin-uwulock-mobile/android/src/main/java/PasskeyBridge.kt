package app.uwulock.mobile

import android.content.Context
import android.content.pm.SigningInfo
import android.os.Build
import android.util.Base64
import android.util.Log
import androidx.annotation.RequiresApi
import androidx.credentials.provider.CallingAppInfo
import org.json.JSONArray
import org.json.JSONObject
import java.security.MessageDigest

/**
 * The way into the app's Rust for Credential Manager (passkeys/android.rs in the app): the open
 * vault lives there, in this process. Every call takes and answers JSON; an answer with "error"
 * failed. When UwULock's library isn't loaded or the app never started, Rust isn't there to ask
 * and the vault counts as locked.
 */
object PasskeyBridge {
    private const val TAG = "UwULock"

    @Volatile
    private var loaded = false

    @JvmStatic
    private external fun nativeCall(method: String, argument: String): String

    private fun load(): Boolean {
        if (loaded) return true
        return try {
            System.loadLibrary("uwulock_desktop_lib")
            loaded = true
            true
        } catch (error: Throwable) {
            Log.w(TAG, "UwULock's library isn't there: $error")
            false
        }
    }

    fun call(method: String, argument: JSONObject = JSONObject()): JSONObject {
        if (!load()) return JSONObject().put("error", "not-running")
        return try {
            JSONObject(nativeCall(method, argument.toString()))
        } catch (error: Throwable) {
            Log.w(TAG, "passkey call $method failed: $error")
            JSONObject().put("error", error.toString())
        }
    }

    /** UwULock runs in this process and its vault is open. */
    fun unlocked(): Boolean = call("status").optBoolean("unlocked", false)

    fun b64(bytes: ByteArray): String =
        Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP)

    /** SHA-256 of the calling app's current signing certificates, URL-safe base64. */
    fun certHashes(signing: SigningInfo): JSONArray {
        val out = JSONArray()
        val digest = MessageDigest.getInstance("SHA-256")
        for (signature in signing.apkContentsSigners) {
            out.put(b64(digest.digest(signature.toByteArray())))
        }
        return out
    }

    /**
     * Who asks: the browser's web origin when it is on the privileged list, the app (package and
     * certificates, for its Digital Asset Links) otherwise.
     */
    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun caller(context: Context, info: CallingAppInfo, requestJson: String, clientDataHash: ByteArray?): JSONObject {
        val args = JSONObject()
            .put("requestJson", requestJson)
            .put("packageName", info.packageName)
            .put("certHashes", certHashes(info.signingInfo))
        try {
            val allowlist = context.resources.openRawResource(R.raw.privileged_browsers)
                .bufferedReader().use { it.readText() }
            val origin = info.getOrigin(allowlist)
            if (!origin.isNullOrEmpty()) {
                args.put("origin", origin.trimEnd('/'))
                if (clientDataHash != null) args.put("clientDataHash", b64(clientDataHash))
            }
        } catch (error: Exception) {
            // A browser that claims an origin without being on the list: treated as an app.
            Log.w(TAG, "no privileged origin for ${info.packageName}: $error")
        }
        return args
    }

    /**
     * The passkeys for a request's site — item id, credential id, names — when the caller may use
     * them (Rust checks the origin or the site's Digital Asset Links first). `null` when the vault
     * isn't open. Without [info] (Android didn't say who asks) only the site is checked.
     */
    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun list(context: Context, info: CallingAppInfo?, requestJson: String): JSONArray? {
        val args = if (info != null) {
            caller(context, info, requestJson, null)
        } else {
            JSONObject().put("requestJson", requestJson).put("unknownCaller", true)
        }
        val answer = call("list", args)
        if (answer.has("error") || answer.optBoolean("locked", true)) return null
        return answer.optJSONArray("passkeys") ?: JSONArray()
    }

    /**
     * Whether the request wants the person verified first. Rust decides (and refuses a request
     * that wanted it without it), so the two can't read the request differently; when Rust can't
     * say, it does.
     */
    fun wantsVerification(requestJson: String, create: Boolean): Boolean {
        val answer = call("verification", JSONObject().put("requestJson", requestJson).put("create", create))
        return answer.optBoolean("wanted", true)
    }
}

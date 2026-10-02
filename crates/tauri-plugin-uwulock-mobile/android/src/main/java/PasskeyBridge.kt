package app.uwulock.mobile

import android.content.pm.SigningInfo
import android.util.Base64
import android.util.Log
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

    /** The passkeys for a request's site: item id, credential id, names. */
    fun list(requestJson: String): JSONArray? {
        val answer = call("list", JSONObject().put("requestJson", requestJson))
        if (answer.has("error") || answer.optBoolean("locked", true)) return null
        return answer.optJSONArray("passkeys") ?: JSONArray()
    }
}

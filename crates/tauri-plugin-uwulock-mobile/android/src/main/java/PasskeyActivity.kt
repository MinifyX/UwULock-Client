package app.uwulock.mobile

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.widget.Toast
import androidx.annotation.RequiresApi
import androidx.appcompat.app.AppCompatActivity
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.credentials.CreatePublicKeyCredentialRequest
import androidx.credentials.CreatePublicKeyCredentialResponse
import androidx.credentials.GetCredentialResponse
import androidx.credentials.GetPublicKeyCredentialOption
import androidx.credentials.PublicKeyCredential
import androidx.credentials.exceptions.CreateCredentialCancellationException
import androidx.credentials.exceptions.CreateCredentialUnknownException
import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.credentials.exceptions.GetCredentialUnknownException
import androidx.credentials.exceptions.domerrors.InvalidStateError
import androidx.credentials.exceptions.publickeycredential.CreatePublicKeyCredentialDomException
import androidx.credentials.provider.CallingAppInfo
import androidx.credentials.provider.PendingIntentHandler
import org.json.JSONObject

/**
 * What happens after the person picked UwULock in Android's sheet: make a passkey (CREATE), sign
 * with one (GET), or open UwULock to unlock it and hand Android the passkeys then (UNLOCK).
 *
 * Before making or signing, the person proves it's them — fingerprint, face or the screen lock —
 * unless the site said "discouraged". The vault work is Rust's (PasskeyBridge), on a background
 * thread: making a passkey saves it to the server first.
 */
@RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
class PasskeyActivity : AppCompatActivity() {
    companion object {
        private const val TAG = "UwULock"
        const val MODE = "app.uwulock.passkeys.MODE"
        const val ITEM_ID = "app.uwulock.passkeys.ITEM_ID"
        const val CREDENTIAL_ID = "app.uwulock.passkeys.CREDENTIAL_ID"
        const val CREATE = "create"
        const val GET = "get"
        const val UNLOCK = "unlock"

        fun intent(context: Context, mode: String): Intent =
            Intent(context, PasskeyActivity::class.java).putExtra(MODE, mode)
    }

    /** UwULock was opened to unlock; the next resume carries on. */
    private var waitingForUnlock = false
    private var started = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (savedInstanceState != null) waitingForUnlock = savedInstanceState.getBoolean("waiting")
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putBoolean("waiting", waitingForUnlock)
    }

    override fun onResume() {
        super.onResume()
        if (started && !waitingForUnlock) return
        started = true
        if (!PasskeyBridge.unlocked()) {
            if (waitingForUnlock) {
                // Back without unlocking.
                Toast.makeText(this, R.string.uwulock_passkeys_locked, Toast.LENGTH_LONG).show()
                cancel()
                return
            }
            openUwULock()
            return
        }
        waitingForUnlock = false
        when (intent.mode()) {
            CREATE -> create()
            GET -> get()
            UNLOCK -> unlocked()
            else -> finish()
        }
    }

    private fun openUwULock() {
        val launch = packageManager.getLaunchIntentForPackage(packageName)
        if (launch == null) {
            cancel()
            return
        }
        waitingForUnlock = true
        startActivity(launch)
    }

    private fun cancel() {
        val result = Intent()
        when (intent.mode()) {
            CREATE -> PendingIntentHandler.setCreateCredentialException(
                result, CreateCredentialCancellationException("UwULock is locked."))
            GET -> PendingIntentHandler.setGetCredentialException(
                result, GetCredentialCancellationException("UwULock is locked."))
            else -> {
                setResult(Activity.RESULT_CANCELED)
                finish()
                return
            }
        }
        setResult(Activity.RESULT_OK, result)
        finish()
    }

    /** The vault is open now: Android gets the passkeys it asked for before. */
    private fun unlocked() {
        val request = PendingIntentHandler.retrieveBeginGetCredentialRequest(intent)
        val response = request?.let { PasskeyProviderService.entries(this, it) }
        if (response == null) {
            cancel()
            return
        }
        val result = Intent()
        PendingIntentHandler.setBeginGetCredentialResponse(result, response)
        setResult(Activity.RESULT_OK, result)
        finish()
    }

    /** Who asks: the browser's web origin when it is on the privileged list, the app otherwise. */
    private fun caller(info: CallingAppInfo, requestJson: String, clientDataHash: ByteArray?): JSONObject {
        val args = JSONObject()
            .put("requestJson", requestJson)
            .put("packageName", info.packageName)
            .put("certHashes", PasskeyBridge.certHashes(info.signingInfo))
        try {
            val allowlist = resources.openRawResource(R.raw.privileged_browsers)
                .bufferedReader().use { it.readText() }
            val origin = info.getOrigin(allowlist)
            if (!origin.isNullOrEmpty()) {
                args.put("origin", origin.trimEnd('/'))
                if (clientDataHash != null) args.put("clientDataHash", PasskeyBridge.b64(clientDataHash))
            }
        } catch (error: Exception) {
            // A browser that claims an origin without being on the list: treated as an app.
            Log.w(TAG, "no privileged origin for ${info.packageName}: $error")
        }
        return args
    }

    private fun wantsVerification(requestJson: String, create: Boolean): Boolean {
        val json = try { JSONObject(requestJson) } catch (error: Exception) { return true }
        val preference = if (create) {
            json.optJSONObject("authenticatorSelection")?.optString("userVerification")
        } else {
            json.optString("userVerification")
        }
        return preference != "discouraged"
    }

    private fun site(requestJson: String): String = try {
        val json = JSONObject(requestJson)
        json.optString("rpId").ifEmpty { json.optJSONObject("rp")?.optString("id") ?: "" }
    } catch (error: Exception) {
        ""
    }

    /** Fingerprint, face or screen lock; `then(true)` when it was asked and passed. */
    private fun verify(wanted: Boolean, subtitle: String, then: (Boolean) -> Unit, failed: () -> Unit) {
        if (!wanted) {
            then(false)
            return
        }
        val prompt = BiometricPrompt(
            this,
            ContextCompat.getMainExecutor(this),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    then(true)
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    failed()
                }
            },
        )
        prompt.authenticate(
            BiometricPrompt.PromptInfo.Builder()
                .setTitle(getString(R.string.uwulock_passkeys_verify))
                .setSubtitle(subtitle)
                .setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
                .build(),
        )
    }

    private fun background(work: () -> Unit) {
        Thread(work, "uwulock-passkey").start()
    }

    private fun create() {
        val request = PendingIntentHandler.retrieveProviderCreateCredentialRequest(intent)
        val calling = request?.callingRequest as? CreatePublicKeyCredentialRequest
        if (request == null || calling == null) {
            finish()
            return
        }
        val args = caller(request.callingAppInfo, calling.requestJson, calling.clientDataHash)
        verify(
            wantsVerification(calling.requestJson, true),
            getString(R.string.uwulock_passkeys_verify_create, site(calling.requestJson)),
            then = { verified ->
                args.put("verified", verified)
                background {
                    val answer = PasskeyBridge.call("create", args)
                    runOnUiThread {
                        val result = Intent()
                        val response = answer.optJSONObject("response")
                        if (response != null) {
                            PendingIntentHandler.setCreateCredentialResponse(
                                result, CreatePublicKeyCredentialResponse(response.toString()))
                        } else {
                            val error = answer.optString("error")
                            PendingIntentHandler.setCreateCredentialException(
                                result,
                                if (error == "excluded") {
                                    CreatePublicKeyCredentialDomException(
                                        InvalidStateError(), "A passkey for this account is in the vault already.")
                                } else {
                                    CreateCredentialUnknownException(error)
                                },
                            )
                        }
                        setResult(Activity.RESULT_OK, result)
                        finish()
                    }
                }
            },
            failed = {
                val result = Intent()
                PendingIntentHandler.setCreateCredentialException(
                    result, CreateCredentialCancellationException("Not confirmed."))
                setResult(Activity.RESULT_OK, result)
                finish()
            },
        )
    }

    private fun get() {
        val request = PendingIntentHandler.retrieveProviderGetCredentialRequest(intent)
        val option = request?.credentialOptions?.filterIsInstance<GetPublicKeyCredentialOption>()?.firstOrNull()
        if (request == null || option == null) {
            finish()
            return
        }
        val args = caller(request.callingAppInfo, option.requestJson, option.clientDataHash)
            .put("itemId", intent.getStringExtra(ITEM_ID))
            .put("credentialId", intent.getStringExtra(CREDENTIAL_ID))
        verify(
            wantsVerification(option.requestJson, false),
            getString(R.string.uwulock_passkeys_verify_get, site(option.requestJson)),
            then = { verified ->
                args.put("verified", verified)
                background {
                    val answer = PasskeyBridge.call("get", args)
                    runOnUiThread {
                        val result = Intent()
                        val response = answer.optJSONObject("response")
                        if (response != null) {
                            PendingIntentHandler.setGetCredentialResponse(
                                result, GetCredentialResponse(PublicKeyCredential(response.toString())))
                        } else {
                            PendingIntentHandler.setGetCredentialException(
                                result, GetCredentialUnknownException(answer.optString("error")))
                        }
                        setResult(Activity.RESULT_OK, result)
                        finish()
                    }
                }
            },
            failed = {
                val result = Intent()
                PendingIntentHandler.setGetCredentialException(
                    result, GetCredentialCancellationException("Not confirmed."))
                setResult(Activity.RESULT_OK, result)
                finish()
            },
        )
    }
}

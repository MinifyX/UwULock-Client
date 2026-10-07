package app.uwulock.mobile

import android.app.Activity
import android.app.assist.AssistStructure
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.service.autofill.Dataset
import android.view.autofill.AutofillManager
import android.view.autofill.AutofillValue
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat

/**
 * The autofill service's second step: the person picked a UwULock suggestion (FILL) or "Unlock
 * UwULock" (UNLOCK).
 *
 * FILL verifies the person — every time, screen lock or a strong biometric — then asks Rust for
 * that login's password, for the same app or site (read again from the screen Android hands over,
 * not from the suggestion), and gives Android the filled values. UNLOCK opens UwULock, waits until
 * it is unlocked and answers with the suggestions.
 */
class FillActivity : AppCompatActivity() {
    companion object {
        const val MODE = "app.uwulock.autofill.MODE"
        const val ITEM_ID = "app.uwulock.autofill.ITEM_ID"
        const val FILL = "fill"
        const val UNLOCK = "unlock"

        fun intent(context: Context, mode: String): Intent =
            Intent(context, FillActivity::class.java).putExtra(MODE, mode)
    }

    private var waitingForUnlock = false
    private var started = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (savedInstanceState != null) {
            waitingForUnlock = savedInstanceState.getBoolean("waiting")
            started = savedInstanceState.getBoolean("started")
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putBoolean("waiting", waitingForUnlock)
        outState.putBoolean("started", started)
    }

    override fun onResume() {
        super.onResume()
        if (started && !waitingForUnlock) return
        started = true
        if (!PasskeyBridge.unlocked()) {
            if (waitingForUnlock) {
                Toast.makeText(this, R.string.uwulock_passkeys_locked, Toast.LENGTH_LONG).show()
                cancel()
                return
            }
            openUwULock()
            return
        }
        waitingForUnlock = false
        val fields = fields()
        if (fields == null) {
            cancel()
            return
        }
        when (intent.getStringExtra(MODE)) {
            FILL -> fill(fields)
            UNLOCK -> unlocked(fields)
            else -> cancel()
        }
    }

    private fun fields(): FillFields? {
        val structure: AssistStructure? = if (Build.VERSION.SDK_INT >= 33) {
            intent.getParcelableExtra(AutofillManager.EXTRA_ASSIST_STRUCTURE, AssistStructure::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(AutofillManager.EXTRA_ASSIST_STRUCTURE)
        }
        val fields = structure?.let { FillFields.parse(it) } ?: return null
        return fields.takeIf { it.packageName != packageName && it.ids.isNotEmpty() }
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
        setResult(Activity.RESULT_CANCELED)
        finish()
    }

    private fun background(work: () -> Unit) {
        Thread(work, "uwulock-autofill").start()
    }

    private fun unlocked(fields: FillFields) {
        background {
            val logins = try {
                LoginBridge.logins(LoginBridge.autofillCaller(this, fields))
            } catch (error: Exception) {
                null
            }
            runOnUiThread {
                when {
                    logins == null -> cancel()
                    logins.isEmpty() -> {
                        Toast.makeText(this, R.string.uwulock_autofill_none, Toast.LENGTH_LONG).show()
                        cancel()
                    }
                    else -> {
                        val result = Intent().putExtra(
                            AutofillManager.EXTRA_AUTHENTICATION_RESULT,
                            UwuLockAutofillService.response(this, fields, logins),
                        )
                        setResult(Activity.RESULT_OK, result)
                        finish()
                    }
                }
            }
        }
    }

    private fun fill(fields: FillFields) {
        val itemId = intent.getStringExtra(ITEM_ID)
        if (itemId.isNullOrEmpty()) {
            cancel()
            return
        }
        val prompt = BiometricPrompt(
            this,
            ContextCompat.getMainExecutor(this),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    background { filled(fields, itemId) }
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    cancel()
                }
            },
        )
        prompt.authenticate(
            BiometricPrompt.PromptInfo.Builder()
                .setTitle(getString(R.string.uwulock_passkeys_verify))
                .setSubtitle(getString(R.string.uwulock_passwords_verify_fill, fields.shownAs))
                .setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
                .build(),
        )
    }

    /** After the person was verified: the values, for this screen's app or site only (Rust checks). */
    @Suppress("DEPRECATION")
    private fun filled(fields: FillFields, itemId: String) {
        val login = try {
            LoginBridge.password(LoginBridge.autofillCaller(this, fields), itemId)
        } catch (error: Exception) {
            null
        }
        runOnUiThread {
            if (login == null) {
                Toast.makeText(this, R.string.uwulock_passwords_failed, Toast.LENGTH_LONG).show()
                cancel()
                return@runOnUiThread
            }
            val dataset = Dataset.Builder(
                UwuLockAutofillService.presentation(this, login.userName.ifEmpty { fields.shownAs }, null),
            )
            var any = false
            val username = fields.username
            if (username != null && login.userName.isNotEmpty()) {
                dataset.setValue(username, AutofillValue.forText(login.userName))
                any = true
            }
            val password = fields.password
            if (password != null) {
                dataset.setValue(password, AutofillValue.forText(login.password))
                any = true
            }
            if (!any) {
                cancel()
                return@runOnUiThread
            }
            setResult(Activity.RESULT_OK, Intent().putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, dataset.build()))
            finish()
        }
    }
}

package app.uwulock.mobile

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.CancellationSignal
import android.os.OutcomeReceiver
import androidx.annotation.RequiresApi
import androidx.credentials.exceptions.ClearCredentialException
import androidx.credentials.exceptions.CreateCredentialException
import androidx.credentials.exceptions.CreateCredentialUnknownException
import androidx.credentials.exceptions.GetCredentialException
import androidx.credentials.provider.AuthenticationAction
import androidx.credentials.provider.BeginCreateCredentialRequest
import androidx.credentials.provider.BeginCreateCredentialResponse
import androidx.credentials.provider.BeginCreatePublicKeyCredentialRequest
import androidx.credentials.provider.BeginGetCredentialRequest
import androidx.credentials.provider.BeginGetCredentialResponse
import androidx.credentials.provider.BeginGetPublicKeyCredentialOption
import androidx.credentials.provider.CreateEntry
import androidx.credentials.provider.CredentialEntry
import androidx.credentials.provider.CredentialProviderService
import androidx.credentials.provider.ProviderClearCredentialStateRequest
import androidx.credentials.provider.PublicKeyCredentialEntry

/**
 * UwULock as a passkey provider for Credential Manager (Android 14+). Android asks here first,
 * quickly and without UI: which passkeys there are for a site, whether UwULock can make one.
 * Picking one starts [PasskeyActivity], which verifies the person and signs or makes.
 *
 * The vault is the app's open one; a locked or closed UwULock offers "Unlock UwULock" only.
 */
@RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
class PasskeyProviderService : CredentialProviderService() {
    companion object {
        /** The entries for a sign-in request, or `null` when the vault isn't open. */
        fun entries(context: Context, request: BeginGetCredentialRequest): BeginGetCredentialResponse? {
            val response = BeginGetCredentialResponse.Builder()
            var index = 0
            for (option in request.beginGetCredentialOptions) {
                if (option !is BeginGetPublicKeyCredentialOption) continue
                val passkeys = PasskeyBridge.list(option.requestJson) ?: return null
                val entries = mutableListOf<CredentialEntry>()
                for (i in 0 until passkeys.length()) {
                    val passkey = passkeys.getJSONObject(i)
                    val userName = passkey.optString("userName").ifEmpty { passkey.optString("itemName") }
                    val intent = PasskeyActivity.intent(context, PasskeyActivity.GET)
                        .putExtra(PasskeyActivity.ITEM_ID, passkey.optString("itemId"))
                        .putExtra(PasskeyActivity.CREDENTIAL_ID, passkey.optString("credentialId"))
                    val pending = PendingIntent.getActivity(
                        context,
                        1000 + index++,
                        intent,
                        PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                    )
                    val entry = PublicKeyCredentialEntry.Builder(context, userName, pending, option)
                    val display = passkey.optString("userDisplayName")
                    if (display.isNotEmpty() && display != "null") entry.setDisplayName(display)
                    entries.add(entry.build())
                }
                entries.forEach { response.addCredentialEntry(it) }
            }
            return response.build()
        }

        fun unlockAction(context: Context): AuthenticationAction {
            val pending = PendingIntent.getActivity(
                context,
                1,
                PasskeyActivity.intent(context, PasskeyActivity.UNLOCK),
                PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            return AuthenticationAction(context.getString(R.string.uwulock_passkeys_unlock), pending)
        }
    }

    override fun onBeginCreateCredentialRequest(
        request: BeginCreateCredentialRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<BeginCreateCredentialResponse, CreateCredentialException>,
    ) {
        if (request !is BeginCreatePublicKeyCredentialRequest) {
            callback.onError(CreateCredentialUnknownException("UwULock only keeps passkeys here."))
            return
        }
        val pending = PendingIntent.getActivity(
            this,
            2,
            PasskeyActivity.intent(this, PasskeyActivity.CREATE),
            PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val entry = CreateEntry.Builder(getString(R.string.uwulock_passkeys_create), pending)
            .setDescription(getString(R.string.uwulock_passkeys_create_hint))
            .build()
        callback.onResult(BeginCreateCredentialResponse.Builder().addCreateEntry(entry).build())
    }

    override fun onBeginGetCredentialRequest(
        request: BeginGetCredentialRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<BeginGetCredentialResponse, GetCredentialException>,
    ) {
        val response = entries(this, request)
            ?: BeginGetCredentialResponse.Builder().addAuthenticationAction(unlockAction(this)).build()
        callback.onResult(response)
    }

    override fun onClearCredentialStateRequest(
        request: ProviderClearCredentialStateRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<Void?, ClearCredentialException>,
    ) {
        // UwULock keeps no sign-in state of its own for other apps.
        callback.onResult(null)
    }
}

internal fun Intent.mode(): String? = getStringExtra(PasskeyActivity.MODE)

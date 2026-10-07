package app.uwulock.mobile

import android.app.PendingIntent
import android.content.Context
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.Dataset
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.SaveCallback
import android.service.autofill.SaveRequest
import android.util.Log
import android.widget.RemoteViews

/**
 * UwULock as Android's autofill service, for apps and browsers that don't ask Credential Manager
 * for passwords. Switched on by the person in Settings (Passwords, passkeys and accounts →
 * Preferred service, or Autofill service on older Android).
 *
 * No password ever goes into a suggestion: each one only carries the login's name and user name,
 * and picking it starts [FillActivity], which verifies the person (screen lock or a strong
 * biometric) and only then hands Android the filled values. A locked or closed UwULock offers
 * "Unlock UwULock" instead; nothing matching offers nothing. Saving isn't offered (no SaveInfo).
 */
class UwuLockAutofillService : AutofillService() {
    companion object {
        private const val TAG = "UwULock"

        /** At most this many logins in the dropdown. */
        private const val LIMIT = 20

        fun presentation(context: Context, title: String, subtitle: String?): RemoteViews =
            RemoteViews(context.packageName, R.layout.uwulock_autofill_item).apply {
                setTextViewText(R.id.uwulock_autofill_title, title)
                setTextViewText(R.id.uwulock_autofill_subtitle, subtitle ?: "")
            }

        /** One suggestion per login, each locked behind [FillActivity] (values come only from there). */
        @Suppress("DEPRECATION")
        fun response(context: Context, fields: FillFields, logins: List<LoginBridge.Login>): FillResponse {
            val response = FillResponse.Builder()
            logins.take(LIMIT).forEachIndexed { index, login ->
                val intent = FillActivity.intent(context, FillActivity.FILL)
                    .putExtra(FillActivity.ITEM_ID, login.itemId)
                // Mutable: Android adds the screen's structure when the person picks it.
                val sender = PendingIntent.getActivity(
                    context,
                    3000 + index,
                    intent,
                    PendingIntent.FLAG_CANCEL_CURRENT or PendingIntent.FLAG_MUTABLE,
                ).intentSender
                val dataset = Dataset.Builder(presentation(context, login.userName ?: login.name, login.name))
                fields.ids.forEach { dataset.setValue(it, null) }
                dataset.setAuthentication(sender)
                response.addDataset(dataset.build())
            }
            return response.build()
        }

        /** "Unlock UwULock": opens the app, then answers with the suggestions. */
        @Suppress("DEPRECATION")
        fun unlockResponse(context: Context, fields: FillFields): FillResponse {
            val sender = PendingIntent.getActivity(
                context,
                2999,
                FillActivity.intent(context, FillActivity.UNLOCK),
                PendingIntent.FLAG_CANCEL_CURRENT or PendingIntent.FLAG_MUTABLE,
            ).intentSender
            return FillResponse.Builder()
                .setAuthentication(
                    fields.ids,
                    sender,
                    presentation(
                        context,
                        context.getString(R.string.uwulock_passkeys_unlock),
                        context.getString(R.string.uwulock_autofill_unlock_hint),
                    ),
                )
                .build()
        }

        /** The answer for a screen; `null` for nothing to offer. May fetch Digital Asset Links. */
        fun answer(context: Context, fields: FillFields): FillResponse? {
            val logins = LoginBridge.logins(LoginBridge.autofillCaller(context, fields))
                ?: return unlockResponse(context, fields)
            if (logins.isEmpty()) return null
            return response(context, fields, logins)
        }
    }

    override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
        val structure = request.fillContexts.lastOrNull()?.structure
        val fields = structure?.let { FillFields.parse(it) }
        if (fields == null || fields.packageName == packageName || fields.ids.isEmpty()) {
            callback.onSuccess(null)
            return
        }
        val context: Context = this
        // Off the main thread: listing may fetch a site's Digital Asset Links.
        Thread(Runnable {
            val response = try {
                answer(context, fields)
            } catch (error: Exception) {
                Log.w(TAG, "autofill: $error")
                null
            }
            if (cancellationSignal.isCanceled) return@Runnable
            try {
                callback.onSuccess(response)
            } catch (error: Exception) {
                // Android gave up on the request meanwhile.
                Log.w(TAG, "autofill answer too late: $error")
            }
        }, "uwulock-autofill").start()
    }

    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
        // Never asked for (no SaveInfo): new logins are saved in UwULock itself.
        callback.onFailure(getString(R.string.uwulock_autofill_no_save))
    }
}

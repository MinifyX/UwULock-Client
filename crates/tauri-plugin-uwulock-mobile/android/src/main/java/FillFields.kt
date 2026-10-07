package app.uwulock.mobile

import android.app.assist.AssistStructure
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId

/**
 * The sign-in fields of a screen another app asks the autofill service to fill: user name and
 * password, and who shows them — the app's package and, in a browser or WebView, the page's
 * domain (of the frame the fields sit in).
 *
 * What a field is: the app's own autofill hints first (`password`, `username`, `emailAddress`, a
 * page's `autocomplete`), then the input type (password variations, e-mail), then words in its id
 * or hint ("pass", "user", "email", "login"). A field for a new password is never filled. Without
 * a user name field, the text field right before the password is taken.
 */
class FillFields(
    val packageName: String,
    val webDomain: String?,
    val webScheme: String?,
    val username: AutofillId?,
    val password: AutofillId?,
) {
    val ids: Array<AutofillId> get() = listOfNotNull(username, password).toTypedArray()

    /** The site or app to name when asking the person. */
    val shownAs: String get() = webDomain ?: packageName

    private enum class Kind { USERNAME, PASSWORD, NEW_PASSWORD, OTHER, SKIP }

    /** A page (frame) a field sits in: its domain and scheme, from the nearest node naming one. */
    private data class Page(val domain: String, val scheme: String?)

    private class Scan {
        var username: AutofillId? = null
        var usernamePage: Page? = null
        var usernameStrong = false
        var password: AutofillId? = null
        var passwordPage: Page? = null
        var lastText: AutofillId? = null
        var lastTextPage: Page? = null
        var beforePassword: AutofillId? = null
        var beforePasswordPage: Page? = null
    }

    companion object {
        fun parse(structure: AssistStructure): FillFields? {
            val packageName = structure.activityComponent?.packageName ?: return null
            val scan = Scan()
            for (i in 0 until structure.windowNodeCount) {
                visit(structure.getWindowNodeAt(i).rootViewNode, scan, null)
            }
            var username = scan.username
            var usernamePage = scan.usernamePage
            if (username == null) {
                username = scan.beforePassword
                usernamePage = scan.beforePasswordPage
            }
            // A user name alone only when the app or page says so (a first sign-in step).
            if (scan.password == null && !(scan.username != null && scan.usernameStrong)) return null
            // The site is the one of the frame the fields sit in, not the first one on the screen:
            // a page's embedded frame from another site must not get the outer site's logins.
            val page = if (scan.password != null) scan.passwordPage else usernamePage
            // Fields of two different sites (or a site and the app) are never filled together.
            if (username != null && scan.password != null && usernamePage != scan.passwordPage) {
                username = null
            }
            return FillFields(packageName, page?.domain, page?.scheme, username, scan.password)
        }

        private fun visit(node: AssistStructure.ViewNode, scan: Scan, inherited: Page?) {
            val domain = node.webDomain
            val page = if (!domain.isNullOrEmpty()) Page(domain.lowercase(), node.webScheme?.lowercase()) else inherited
            val id = node.autofillId
            if (id != null && node.autofillType == View.AUTOFILL_TYPE_TEXT && node.visibility == View.VISIBLE) {
                when (kind(node)) {
                    Kind.PASSWORD -> if (scan.password == null) {
                        scan.password = id
                        scan.passwordPage = page
                        scan.beforePassword = scan.lastText
                        scan.beforePasswordPage = scan.lastTextPage
                    }
                    Kind.USERNAME -> if (scan.username == null && scan.password == null) {
                        scan.username = id
                        scan.usernamePage = page
                        scan.usernameStrong = strongUsername(node)
                    }
                    Kind.OTHER -> if (scan.password == null) {
                        scan.lastText = id
                        scan.lastTextPage = page
                    }
                    Kind.NEW_PASSWORD, Kind.SKIP -> {}
                }
            }
            for (i in 0 until node.childCount) visit(node.getChildAt(i), scan, page)
        }

        private fun hints(node: AssistStructure.ViewNode): List<String> {
            val out = mutableListOf<String>()
            node.autofillHints?.forEach { out.add(it.lowercase()) }
            node.htmlInfo?.attributes?.forEach { attribute ->
                if (attribute.first.equals("autocomplete", ignoreCase = true)) {
                    attribute.second?.lowercase()?.split(' ')?.let { out.addAll(it) }
                }
            }
            return out
        }

        private fun htmlType(node: AssistStructure.ViewNode): String? =
            node.htmlInfo?.attributes?.firstOrNull { it.first.equals("type", ignoreCase = true) }?.second?.lowercase()

        private fun strongUsername(node: AssistStructure.ViewNode): Boolean {
            val hints = hints(node)
            if (hints.any { it == "username" || it == "email" || it == "emailaddress" }) return true
            val variation = node.inputType and InputType.TYPE_MASK_VARIATION
            return htmlType(node) == "email" ||
                ((node.inputType and InputType.TYPE_MASK_CLASS) == InputType.TYPE_CLASS_TEXT &&
                    (variation == InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS ||
                        variation == InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS))
        }

        private fun kind(node: AssistStructure.ViewNode): Kind {
            val hints = hints(node)
            if (hints.any { it.contains("new-password") || it.contains("newpassword") }) return Kind.NEW_PASSWORD
            if (hints.any { it == "password" || it == "current-password" }) return Kind.PASSWORD
            if (hints.any { it == "username" || it == "email" || it == "emailaddress" }) return Kind.USERNAME

            val type = htmlType(node)
            if (type != null && type in setOf("hidden", "submit", "button", "checkbox", "radio", "search")) {
                return Kind.SKIP
            }
            if (type == "password") return Kind.PASSWORD
            if (type == "email") return Kind.USERNAME

            val textClass = (node.inputType and InputType.TYPE_MASK_CLASS) == InputType.TYPE_CLASS_TEXT
            val variation = node.inputType and InputType.TYPE_MASK_VARIATION
            if (textClass && variation in setOf(
                    InputType.TYPE_TEXT_VARIATION_PASSWORD,
                    InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD,
                    InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD,
                )
            ) {
                return Kind.PASSWORD
            }
            if (textClass && (variation == InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS ||
                    variation == InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS)
            ) {
                return Kind.USERNAME
            }

            val words = listOfNotNull(
                node.idEntry,
                node.hint,
                node.htmlInfo?.attributes?.firstOrNull { it.first.equals("name", ignoreCase = true) }?.second,
                node.htmlInfo?.attributes?.firstOrNull { it.first.equals("id", ignoreCase = true) }?.second,
            ).joinToString(" ").lowercase()
            if (words.contains("pass") || words.contains("kennwort")) {
                return if (listOf("new", "neu", "confirm", "repeat").any { words.contains(it) }) Kind.NEW_PASSWORD else Kind.PASSWORD
            }
            if (listOf("user", "email", "e-mail", "login", "benutzer").any { words.contains(it) }) return Kind.USERNAME
            return Kind.OTHER
        }
    }
}

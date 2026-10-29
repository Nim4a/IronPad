package com.nim4a.ironpad

import android.app.AlertDialog
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.graphics.Color
import android.graphics.pdf.PdfDocument
import android.net.Uri
import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.text.style.ForegroundColorSpan
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.widget.Toolbar
import androidx.activity.result.contract.ActivityResultContracts

class MainActivity : AppCompatActivity() {
    private lateinit var editor: EditText
    private lateinit var lineNumbers: TextView
    private lateinit var status: TextView
    private lateinit var tabStrip: LinearLayout
    private lateinit var toolbar: Toolbar

    private val tabs = mutableListOf("untitled")
    private var active = 0
    private val texts = mutableMapOf<Int, String>()
    private var darkMode = true
    private var currentUri: Uri? = null
    private var findLastIndex = 0
    private var theme = "dark" // dark | light | sepia

    private val openLauncher = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        uri?.let { readUri(it) }
    }
    private val saveAsLauncher = registerForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri: Uri? ->
        uri?.let { writeUri(it, makeCurrent = true) }
    }
    private val pdfLauncher = registerForActivityResult(ActivityResultContracts.CreateDocument("application/pdf")) { uri: Uri? ->
        uri?.let { exportPdfToUri(it) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // load persisted theme
        val prefs = getSharedPreferences("ironpad", Context.MODE_PRIVATE)
        theme = prefs.getString("theme", "dark")!!
        setContentView(R.layout.activity_main)
        editor = findViewById(R.id.editor)
        lineNumbers = findViewById(R.id.lineNumbers)
        status = findViewById(R.id.status)
        tabStrip = findViewById(R.id.tabStrip)
        toolbar = findViewById(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.title = "IronPad"


        if (intent?.action == Intent.ACTION_VIEW && intent.data != null) {
            readUri(intent.data!!)
        }

        editor.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                highlight(s ?: return)
                updateLineNumbers()
                updateStatus()
                findLastIndex = 0
            }
        })

        rebuildTabs()
        applyTheme()
        updateLineNumbers()
        updateStatus()
    }

    override fun onCreateOptionsMenu(menu: android.view.Menu): Boolean {
        menuInflater.inflate(R.menu.main_menu, menu)
        val tint = Color.parseColor(if (darkMode) "#E0E0E0" else "#1A1A1A")
        for (i in 0 until menu.size()) {
            menu.getItem(i).icon?.setTint(tint)
        }
        return true
    }

    override fun onOptionsItemSelected(item: android.view.MenuItem): Boolean {
        return when (item.itemId) {
            R.id.action_new -> { newFile(); true }
            R.id.action_open -> { openLauncher.launch(arrayOf("*/*")); true }
            R.id.action_save -> { saveFile(); true }
            R.id.action_save_as -> { saveAsLauncher.launch("untitled.txt"); true }
            R.id.action_share -> { shareText(); true }
            R.id.action_share_file -> { shareFile(); true }
            R.id.action_find -> { showFindReplace(); true }
            R.id.action_theme -> { showThemeSelector(); true }
            R.id.action_export_pdf -> { pdfLauncher.launch("IronPad.pdf"); true }
            else -> super.onOptionsItemSelected(item)
        }
    }

    private fun newFile() {
        texts[active] = editor.text.toString()
        tabs.add("untitled")
        active = tabs.lastIndex
        texts[active] = ""
        currentUri = null
        editor.setText("")
        supportActionBar?.title = "IronPad"
        rebuildTabs()
    }

    private fun saveFile() {
        val uri = currentUri
        if (uri != null) writeUri(uri, makeCurrent = false)
        else saveAsLauncher.launch("untitled.txt")
    }

    private fun shareText() {
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, editor.text.toString())
            putExtra(Intent.EXTRA_SUBJECT, tabs.getOrElse(active) { "untitled" })
        }
        startActivity(Intent.createChooser(send, "Share text"))
    }

    private fun shareFile() {
        val name = (tabs.getOrElse(active) { "untitled" }) + ".txt"
        val file = java.io.File(cacheDir, name)
        file.writeText(editor.text.toString())
        val uri = androidx.core.content.FileProvider.getUriForFile(this, "$packageName.fileprovider", file)
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_STREAM, uri)
            putExtra(Intent.EXTRA_SUBJECT, name)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        startActivity(Intent.createChooser(send, "Share file"))
    }

    private fun showThemeSelector() {
        val items = arrayOf(getString(R.string.theme_dark), getString(R.string.theme_light), getString(R.string.theme_sepia))
        val current = when (theme) {
            "light" -> 1
            "sepia" -> 2
            else -> 0
        }
        AlertDialog.Builder(this)
            .setTitle(getString(R.string.select_theme))
            .setSingleChoiceItems(items, current) { _, which ->
                theme = when (which) {
                    1 -> "light"
                    2 -> "sepia"
                    else -> "dark"
                }
                applyTheme()
            }
            .setPositiveButton("OK") { _, _ -> }
            .create()
            .show()
    }

    private fun exportPdfToUri(uri: Uri) {
        try {
            contentResolver.openOutputStream(uri)?.use { out ->
                val doc = generatePdf()
                doc.writeTo(out)
                doc.close()
            }
        } catch (_: Exception) { }
    }

    private fun generatePdf(): PdfDocument {
        val doc = PdfDocument()
        val text = editor.text.toString()
        val lines = text.split("\n")
        val pageWidth = 595f
        val pageHeight = 842f
        val margin = 50f
        val fontSize = 12f
        val lineHeight = fontSize * 1.4f

        val textColor = when (theme) {
            "light" -> Color.BLACK
            "sepia" -> Color.parseColor("#4A3A28")
            else -> Color.WHITE
        }
        val bgColor = when (theme) {
            "light" -> Color.WHITE
            "sepia" -> Color.parseColor("#F5E6D3")
            else -> Color.parseColor("#181818")
        }
        val paint = android.graphics.Paint().apply {
            color = textColor
            textSize = fontSize
            typeface = android.graphics.Typeface.MONOSPACE
        }
        val bgPaint = android.graphics.Paint().apply { color = bgColor }

        var pageNo = 1
        var page = newPdfPage(doc, pageNo, pageWidth, pageHeight, bgPaint)
        var canvas = page.canvas
        var y = margin
        for (line in lines) {
            if (y + lineHeight > pageHeight - margin) {
                doc.finishPage(page)
                pageNo++
                page = newPdfPage(doc, pageNo, pageWidth, pageHeight, bgPaint)
                canvas = page.canvas
                y = margin
            }
            canvas.drawText(line, margin, y, paint)
            y += lineHeight
        }
        doc.finishPage(page)
        return doc
    }

    private fun newPdfPage(doc: PdfDocument, pageNo: Int, w: Float, h: Float, bgPaint: android.graphics.Paint): PdfDocument.Page {
        val info = PdfDocument.PageInfo.Builder(w.toInt(), h.toInt(), pageNo).create()
        val page = doc.startPage(info)
        page.canvas.drawRect(0f, 0f, w, h, bgPaint)
        return page
    }

    private fun showFindReplace() {
        val findInput = EditText(this).apply { hint = "Find"; setSingleLine(true) }
        val replaceInput = EditText(this).apply { hint = "Replace"; setSingleLine(true) }
        val layout = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), dp(16), dp(16), dp(8))
            addView(findInput)
            addView(replaceInput)
        }
        AlertDialog.Builder(this)
            .setTitle("Find / Replace")
            .setView(layout)
            .setPositiveButton("Find Next") { _, _ ->
                findLastIndex = findNext(findInput.text.toString(), findLastIndex)
            }
            .setNeutralButton("Replace") { _, _ ->
                replaceOne(findInput.text.toString(), replaceInput.text.toString())
            }
            .setNegativeButton("Replace All") { _, _ ->
                replaceAll(findInput.text.toString(), replaceInput.text.toString())
                findLastIndex = 0
            }
            .create()
            .show()
    }

    private fun findNext(needle: String, from: Int): Int {
        if (needle.isEmpty()) return 0
        val text = editor.text.toString()
        var idx = text.indexOf(needle, from)
        if (idx < 0) {
            idx = text.indexOf(needle, 0)
            if (idx < 0) return 0
        }
        editor.setSelection(idx, idx + needle.length)
        return idx + needle.length
    }

    private fun replaceOne(needle: String, replacement: String) {
        val start = editor.selectionStart
        val end = editor.selectionEnd
        if (start != end && editor.text.toString().substring(start, end) == needle) {
            editor.text.replace(start, end, replacement)
        }
    }

    private fun replaceAll(needle: String, replacement: String) {
        if (needle.isEmpty()) return
        val text = editor.text.toString()
        val replaced = text.replace(needle, replacement)
        if (replaced != text) {
            editor.text.clear()
            editor.text.append(replaced)
        }
    }

    private fun dp(px: Int): Int = (px * resources.displayMetrics.density + 0.5f).toInt()

    private fun readUri(uri: Uri) {
        try {
            contentResolver.openInputStream(uri)?.use { stream ->
                editor.setText(stream.bufferedReader().readText())
                currentUri = uri
                supportActionBar?.title = uri.lastPathSegment ?: "IronPad"
                updateLineNumbers()
                updateStatus()
            }
        } catch (_: Exception) { }
    }

    private fun writeUri(uri: Uri, makeCurrent: Boolean) {
        try {
            contentResolver.openOutputStream(uri)?.use { stream ->
                stream.writer().use { it.write(editor.text.toString()) }
            }
            if (makeCurrent) {
                currentUri = uri
                supportActionBar?.title = uri.lastPathSegment ?: "IronPad"
            }
            texts[active] = editor.text.toString()
            tabs[active] = (currentUri?.lastPathSegment ?: tabs[active])
            rebuildTabs()
        } catch (_: Exception) { }
    }

    private fun rebuildTabs() {
        tabStrip.removeAllViews()
        for (i in tabs.indices) {
            val chip = TextView(this).apply {
                text = tabs[i]
                setTextColor(if (i == active) (if (darkMode) Color.WHITE else Color.parseColor("#1A1A1A")) else Color.parseColor(if (darkMode) "#AAB0BA" else "#555555"))
                textSize = 13f
                setPadding(24, 12, 24, 12)
                setBackgroundColor(if (i == active) Color.parseColor(if (darkMode) "#353A44" else "#D0D0D0") else Color.parseColor(if (darkMode) "#1B1D22" else "#F0F0F0"))
                setOnClickListener { switchTab(i) }
            }
            tabStrip.addView(chip)
            val close = TextView(this).apply {
                text = "  \u00D7"
                setTextColor(Color.parseColor(if (darkMode) "#888E98" else "#6B6B6B"))
                textSize = 13f
                setPadding(4, 12, 12, 12)
                setOnClickListener { closeTab(i) }
            }
            tabStrip.addView(close)
        }
        val plus = TextView(this).apply {
            text = "  +  "
            setTextColor(Color.parseColor(if (darkMode) "#AAB0BA" else "#555555"))
            textSize = 16f
            setPadding(16, 12, 16, 12)
            setOnClickListener { newTab() }
        }
        tabStrip.addView(plus)
        val themeBtn = TextView(this).apply {
            text = if (darkMode) "  ☀  " else "  ☾  "
            setTextColor(Color.parseColor(if (darkMode) "#AAB0BA" else "#555555"))
            textSize = 16f
            setPadding(16, 12, 16, 12)
            setOnClickListener { darkMode = !darkMode; applyTheme(); rebuildTabs() }
        }
        tabStrip.addView(themeBtn)
    }

    private fun switchTab(i: Int) {
        texts[active] = editor.text.toString()
        active = i
        editor.setText(texts[active] ?: "")
        editor.setSelection(editor.text.length)
        rebuildTabs()
    }

    private fun newTab() {
        texts[active] = editor.text.toString()
        tabs.add("untitled")
        active = tabs.lastIndex
        texts[active] = ""
        editor.setText("")
        rebuildTabs()
    }

    private fun closeTab(i: Int) {
        if (tabs.size <= 1) return
        texts.remove(i)
        tabs.removeAt(i)
        if (active >= tabs.size) active = tabs.size - 1
        val remapped = mutableMapOf<Int, String>()
        var ni = 0
        for (k in 0 until tabs.size + 1) {
            if (k == i) continue
            texts[k]?.let { remapped[ni] = it }
            ni++
        }
        texts.clear()
        texts.putAll(remapped)
        editor.setText(texts[active] ?: "")
        rebuildTabs()
    }

    private fun applyTheme() {
        val bg = when (theme) {
            "light" -> "#F5F5F5"
            "sepia" -> "#F5E6D3"
            else -> "#181818"
        }
        val panel = when (theme) {
            "light" -> "#E8E8E8"
            "sepia" -> "#E6D5B8"
            else -> "#23262C"
        }
        val text = when (theme) {
            "light" -> "#1A1A1A"
            "sepia" -> "#4A3A28"
            else -> "#E0E0E0"
        }
        val num = when (theme) {
            "light" -> "#6B6B6B"
            "sepia" -> "#6B5B4B"
            else -> "#888E98"
        }
        // tab strip + its scroll container (fixes the dark corner in light mode)
        val strip = findViewById<android.widget.LinearLayout>(R.id.tabStrip)
        strip.setBackgroundColor(Color.parseColor(panel))
        toolbar.setBackgroundColor(Color.parseColor(panel))
        toolbar.setTitleTextColor(Color.parseColor(text))
        supportActionBar?.title = supportActionBar?.title
        invalidateOptionsMenu()
        (strip.parent as? android.widget.HorizontalScrollView)?.setBackgroundColor(Color.parseColor(panel))
        lineNumbers.setBackgroundColor(Color.parseColor(panel))
        lineNumbers.setTextColor(Color.parseColor(num))
        editor.setBackgroundColor(Color.parseColor(bg))
        editor.setTextColor(Color.parseColor(text))
        status.setBackgroundColor(Color.parseColor(panel))
        status.setTextColor(Color.parseColor(num))
        // system status bar: color + icon contrast
        window.statusBarColor = Color.parseColor(panel)
        val lightStatus = theme == "light" || theme == "sepia"
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
            window.insetsController?.setSystemBarsAppearance(
                if (lightStatus) android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS else 0,
                android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
            )
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = if (lightStatus) android.view.View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR else 0
        }
        // persist
        getSharedPreferences("ironpad", Context.MODE_PRIVATE).edit().putString("theme", theme).apply()
    }

    private fun updateLineNumbers() {
        val n = editor.text.toString().count { it == '\n' } + 1
        lineNumbers.text = (1..n).joinToString("\n")
    }

    private fun updateStatus() {
        val t = editor.text.toString()
        val lines = t.count { it == '\n' } + 1
        val words = t.split(Regex("""\s+""")).count { it.isNotEmpty() }
        val chars = t.length
        status.text = "$lines lines  \u00B7  $words words  \u00B7  $chars chars  \u00B7  ${tabs.size} tabs"
    }

    private fun highlight(e: Editable) {
        val text = e.toString()
        e.getSpans(0, e.length, ForegroundColorSpan::class.java).forEach { e.removeSpan(it) }
        val rules = listOf(
            "#808080" to Regex("""//.*|#.*"""),
            "#C586C0" to Regex("""\b(fun|val|var|class|object|if|else|for|while|return|import|package|override|private|public|protected|static|final|void|int|long|double|float|boolean|String|Boolean|Int|Long|Float|Double)\b"""),
            "#CE9178" to Regex(""" "[^"]*"|'[^']*' """),
            "#B5CEA8" to Regex("""\b\d+(\.\d+)?\b""")
        )
        for ((color, regex) in rules) {
            regex.findAll(text).forEach { m ->
                e.setSpan(ForegroundColorSpan(Color.parseColor(color)), m.range.first, m.range.last, android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
        }
    }
}

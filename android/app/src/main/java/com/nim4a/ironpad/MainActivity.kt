package com.nim4a.ironpad

import android.graphics.Color
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

    private val openLauncher = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        uri?.let { readUri(it) }
    }
    private val saveAsLauncher = registerForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri: Uri? ->
        uri?.let { writeUri(it, makeCurrent = true) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        editor = findViewById(R.id.editor)
        lineNumbers = findViewById(R.id.lineNumbers)
        status = findViewById(R.id.status)
        tabStrip = findViewById(R.id.tabStrip)
        toolbar = findViewById(R.id.toolbar)
        setSupportActionBar(toolbar)
        supportActionBar?.title = "IronPad"


        editor.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                highlight(s ?: return)
                updateLineNumbers()
                updateStatus()
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
            R.id.action_open -> { openLauncher.launch(arrayOf("text/*")); true }
            R.id.action_save -> { saveFile(); true }
            R.id.action_save_as -> { saveAsLauncher.launch("untitled.txt"); true }
            R.id.action_theme -> { darkMode = !darkMode; applyTheme(); rebuildTabs(); true }
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

    private fun readUri(uri: Uri) {
        try {
            contentResolver.openInputStream(uri)?.use { stream ->
                editor.setText(stream.bufferedReader().readText())
                currentUri = uri
                supportActionBar?.title = uri.lastPathSegment ?: "IronPad"
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
        val bg = if (darkMode) "#181818" else "#F5F5F5"
        val panel = if (darkMode) "#23262C" else "#E8E8E8"
        val text = if (darkMode) "#E0E0E0" else "#1A1A1A"
        val num = if (darkMode) "#888E98" else "#6B6B6B"
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
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
            window.insetsController?.setSystemBarsAppearance(
                if (darkMode) 0 else android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS,
                android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
            )
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = if (darkMode) 0 else android.view.View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
        }
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

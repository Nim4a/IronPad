package com.nim4a.ironpad

import android.graphics.Color
import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.text.style.ForegroundColorSpan
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity() {
    private lateinit var editor: EditText
    private lateinit var lineNumbers: TextView
    private lateinit var status: TextView
    private lateinit var tabStrip: LinearLayout

    private val tabs = mutableListOf("untitled")
    private var active = 0
    private val texts = mutableMapOf<Int, String>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        editor = findViewById(R.id.editor)
        lineNumbers = findViewById(R.id.lineNumbers)
        status = findViewById(R.id.status)
        tabStrip = findViewById(R.id.tabStrip)

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
        updateLineNumbers()
        updateStatus()
    }

    private fun rebuildTabs() {
        tabStrip.removeAllViews()
        for (i in tabs.indices) {
            val chip = TextView(this).apply {
                text = tabs[i]
                setTextColor(if (i == active) Color.WHITE else Color.parseColor("#AAB0BA"))
                textSize = 13f
                setPadding(24, 12, 24, 12)
                setBackgroundColor(if (i == active) Color.parseColor("#353A44") else Color.parseColor("#1B1D22"))
                setOnClickListener { switchTab(i) }
            }
            tabStrip.addView(chip)
            val close = TextView(this).apply {
                text = "  \u00D7"
                setTextColor(Color.parseColor("#888E98"))
                textSize = 13f
                setPadding(4, 12, 12, 12)
                setOnClickListener { closeTab(i) }
            }
            tabStrip.addView(close)
        }
        val plus = TextView(this).apply {
            text = "  +  "
            setTextColor(Color.parseColor("#AAB0BA"))
            textSize = 16f
            setPadding(16, 12, 16, 12)
            setOnClickListener { newTab() }
        }
        tabStrip.addView(plus)
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

    private fun updateLineNumbers() {
        val n = editor.text.toString().count { it == '\n' } + 1
        lineNumbers.text = (1..n).joinToString("\n")
    }

    private fun updateStatus() {
        val t = editor.text.toString()
        val lines = t.count { it == '\n' } + 1
        val words = t.split(Regex("\s+")).count { it.isNotEmpty() }
        val chars = t.length
        status.text = "$lines lines  \u00B7  $words words  \u00B7  $chars chars  \u00B7  ${tabs.size} tabs"
    }

    private fun highlight(e: Editable) {
        val text = e.toString()
        e.getSpans(0, e.length, ForegroundColorSpan::class.java).forEach { e.removeSpan(it) }
        val rules = listOf(
            "#808080" to Regex("""//.*|#.*"""),
            "#C586C0" to Regex("""\b(fun|val|var|class|object|if|else|for|while|return|import|package|override|private|public|protected|static|final|void|int|long|double|float|boolean|String|Boolean|Int|Long|Float|Double)\b"""),
            "#CE9178" to Regex(""""[^"\]*"|'[^'\]*'"""),
            "#B5CEA8" to Regex("""\b\d+(\.\d+)?\b""")
        )
        for ((color, regex) in rules) {
            regex.findAll(text).forEach { m ->
                e.setSpan(ForegroundColorSpan(Color.parseColor(color)), m.range.first, m.range.last, android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
        }
    }
}

package com.nim4a.ironpad

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity() {
    private lateinit var editor: EditText
    private lateinit var lineNumbers: TextView

    private val openLauncher = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        uri?.let { readUri(it) }
    }
    private val saveLauncher = registerForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri: Uri? ->
        uri?.let { writeUri(it) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        editor = findViewById(R.id.editor)
        lineNumbers = findViewById(R.id.lineNumbers)

        // RTL/Persian is handled automatically by Android's text engine.
        editor.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) { updateLineNumbers() }
        })

        findViewById<Button>(R.id.btnNew).setOnClickListener { editor.setText("") }
        findViewById<Button>(R.id.btnOpen).setOnClickListener { openLauncher.launch(arrayOf("text/*")) }
        findViewById<Button>(R.id.btnSave).setOnClickListener { saveLauncher.launch("untitled.txt") }

        updateLineNumbers()
    }

    private fun updateLineNumbers() {
        val n = editor.text.toString().count { it == '\n' } + 1
        lineNumbers.text = (1..n).joinToString("\n")
    }

    private fun readUri(uri: Uri) {
        try {
            contentResolver.openInputStream(uri)?.use { stream ->
                editor.setText(stream.bufferedReader().readText())
            }
        } catch (_: Exception) { }
    }

    private fun writeUri(uri: Uri) {
        try {
            contentResolver.openOutputStream(uri)?.use { stream ->
                stream.writer().use { it.write(editor.text.toString()) }
            }
        } catch (_: Exception) { }
    }
}

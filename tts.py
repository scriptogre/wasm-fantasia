"""Kokoro TTS wrapper for Venom voice pipeline.

Usage: python tts.py "Text to speak" output.wav
"""
import sys
import soundfile as sf
from kokoro_onnx import Kokoro

kokoro = Kokoro("kokoro-v0_19.onnx", "voices.bin")
text = sys.argv[1]
output_file = sys.argv[2]

# am_adam is a deep male voice in v0.19
samples, sample_rate = kokoro.create(text, voice="am_adam", speed=0.9, lang="en-us")
sf.write(output_file, samples, sample_rate)

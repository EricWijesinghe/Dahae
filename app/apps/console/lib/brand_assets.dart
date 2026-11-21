// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:async';
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:core/core.dart';
import 'package:flutter/material.dart';

/// The brand image slots the server accepts in `business.brand_assets`, with their exact pixel size.
/// The server checks each PNG's IHDR size against this table.
const brandSlots = <String, (int, int)>{
  'favicon_32': (32, 32),
  'apple_180': (180, 180),
  'icon_192': (192, 192),
  'icon_512': (512, 512),
  'maskable_512': (512, 512),
  'og_1200': (1200, 630),
  'store_1024': (1024, 1024),
};

/// Slots drawn on the brand colour (opaque). The others keep the logo's own transparency.
const _filled = {'apple_180', 'maskable_512', 'og_1200', 'store_1024'};

/// Share of the canvas the logo may take, per filled slot.
/// Maskable: 0.56 (0.8 / sqrt 2) keeps a square logo's corners inside the safe circle (radius 0.4).
const _logoShare = {'apple_180': 0.8, 'maskable_512': 0.56, 'store_1024': 0.7};

/// dart:ui takes package fonts by their registered name (packages/ui/pubspec.yaml): a bare family falls back
/// to a system or downloaded font, which the /admin CSP blocks on the web.
const _font = 'packages/ui/Dahae Sans';
const _fontFull = 'packages/ui/Dahae Sans Full';

/// Width and height from a PNG's IHDR chunk (bytes 16-24), or null when the data is too short.
(int, int)? pngSize(Uint8List png) {
  if (png.length < 24) return null;
  final b = ByteData.sublistView(png);
  return (b.getUint32(16), b.getUint32(20));
}

Future<ui.Image> decodeLogo(Uint8List bytes) async {
  final codec = await ui.instantiateImageCodec(bytes);
  final frame = await codec.getNextFrame();
  codec.dispose();
  return frame.image;
}

/// Loads an uploaded logo by URL (same origin as the console) for re-drawing the slots.
/// [current] (the `/brand` logo address) when it names the same file as [saved], else [saved].
String freshLogoUrl(String saved, String? current) {
  final name = Uri.tryParse(saved)?.pathSegments.lastOrNull;
  return current != null && name != null && current.contains(name)
      ? current
      : saved;
}

Future<ui.Image> loadLogo(String url) {
  final done = Completer<ui.Image>();
  final stream = NetworkImage(url).resolve(ImageConfiguration.empty);
  late final ImageStreamListener listener;
  listener = ImageStreamListener(
    (info, _) {
      stream.removeListener(listener);
      done.complete(info.image);
    },
    onError: (e, _) {
      stream.removeListener(listener);
      done.completeError(e);
    },
  );
  stream.addListener(listener);
  return done.future;
}

/// True when Save must make the slot images again: the logo, colour, name or tagline changed, or a logo
/// exists without images. Without any logo there is nothing to draw.
bool needsBrandRemake(Json saved, Json now, {required bool newLogo}) {
  if (newLogo) return true;
  if (now['logo_url'] == null) return false;
  final assets = now['brand_assets'];
  if (assets is! Map || assets.isEmpty) return true;
  return [
    'logo_url',
    'primary_color',
    'name',
    'tagline',
  ].any((k) => (saved[k] ?? '') != (now[k] ?? ''));
}

/// A logo for a business without one: the name's first character, white on the brand colour.
/// It replaces the built-in mark, which belongs to the licensor.
Future<Uint8List> letterMark(String name, Color color) {
  const size = 512.0;
  final first = name.trim().characters.firstOrNull ?? '?';
  return _render(512, 512, (canvas) {
    canvas.drawRect(
      const Rect.fromLTWH(0, 0, size, size),
      Paint()..color = color,
    );
    final p = _paragraph(
      first,
      size: 300,
      weight: FontWeight.w700,
      width: size,
    );
    canvas.drawParagraph(p, Offset(0, (size - p.height) / 2));
  });
}

/// Draws every slot from [logo] on the brand [color]. The share card also carries [name] and [tagline].
Future<Map<String, Uint8List>> deriveBrandAssets({
  required ui.Image logo,
  required Color color,
  required String name,
  String tagline = '',
}) async => {
  for (final MapEntry(key: slot, value: (w, h)) in brandSlots.entries)
    slot: await _render(w, h, (canvas) {
      final area = Rect.fromLTWH(0, 0, w.toDouble(), h.toDouble());
      if (_filled.contains(slot)) canvas.drawRect(area, Paint()..color = color);
      if (slot == 'og_1200') {
        _shareCard(canvas, area, logo, name, tagline);
      } else {
        final share = _logoShare[slot] ?? 1.0;
        _drawContained(
          canvas,
          logo,
          Rect.fromCenter(
            center: area.center,
            width: area.width * share,
            height: area.height * share,
          ),
        );
      }
    }),
};

/// 1200x630: logo on the left, name and tagline on the right, white on the brand colour.
void _shareCard(
  Canvas c,
  Rect area,
  ui.Image logo,
  String name,
  String tagline,
) {
  const pad = 96.0, logoSide = 300.0;
  _drawContained(
    c,
    logo,
    Rect.fromLTWH(pad, (area.height - logoSide) / 2, logoSide, logoSide),
  );
  const left = pad * 2 + logoSide;
  final width = area.width - left - pad;
  final title = _paragraph(
    name,
    size: 88,
    weight: FontWeight.w800,
    width: width,
    align: TextAlign.left,
  );
  final sub = _paragraph(
    tagline,
    size: 44,
    weight: FontWeight.w500,
    width: width,
    align: TextAlign.left,
  );
  final (gap, subHeight) = tagline.isEmpty ? (0.0, 0.0) : (24.0, sub.height);
  final top = (area.height - title.height - gap - subHeight) / 2;
  c.drawParagraph(title, Offset(left, top));
  if (tagline.isNotEmpty) {
    c.drawParagraph(sub, Offset(left, top + title.height + gap));
  }
}

void _drawContained(Canvas c, ui.Image img, Rect box) {
  final scale = math.min(box.width / img.width, box.height / img.height);
  final dst = Rect.fromCenter(
    center: box.center,
    width: img.width * scale,
    height: img.height * scale,
  );
  final src = Rect.fromLTWH(0, 0, img.width.toDouble(), img.height.toDouble());
  c.drawImageRect(img, src, dst, Paint()..filterQuality = FilterQuality.high);
}

ui.Paragraph _paragraph(
  String text, {
  required double size,
  required FontWeight weight,
  required double width,
  TextAlign align = TextAlign.center,
}) {
  final b =
      ui.ParagraphBuilder(
          ui.ParagraphStyle(
            textAlign: align,
            fontFamily: _font,
            fontSize: size,
            fontWeight: weight,
            maxLines: 2,
            ellipsis: '…',
          ),
        )
        ..pushStyle(
          ui.TextStyle(
            color: Colors.white,
            fontFamily: _font,
            fontFamilyFallback: const [_fontFull],
          ),
        )
        ..addText(text);
  return b.build()..layout(ui.ParagraphConstraints(width: width));
}

Future<Uint8List> _render(int w, int h, void Function(Canvas) draw) async {
  final recorder = ui.PictureRecorder();
  draw(Canvas(recorder));
  final picture = recorder.endRecording();
  final image = await picture.toImage(w, h);
  picture.dispose();
  final data = await image.toByteData(format: ui.ImageByteFormat.png);
  image.dispose();
  if (data == null) throw StateError('png_encode_failed');
  return data.buffer.asUint8List();
}

/// Uploads each slot image (one `/uploads` call each) and returns the stored URLs by slot.
Future<Map<String, String>> uploadBrandAssets(
  ApiClient api,
  Map<String, Uint8List> files, {
  void Function(int done, int total)? onProgress,
}) async {
  final urls = <String, String>{};
  for (final MapEntry(key: slot, value: png) in files.entries) {
    onProgress?.call(urls.length, files.length);
    urls[slot] = await api.upload(png, '$slot.png');
  }
  return urls;
}

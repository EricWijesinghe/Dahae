// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import 'api.dart';
import 'foreground.dart';
import 'json_cache.dart';
import 'models.dart';

final _hexColour = RegExp(r'^#[0-9A-Fa-f]{6}$');

/// `/brand` returns uploads signed by the API: `<base>/v1/files/<32 hex>.<ext>?exp=<n>&sig=<hex>`.
/// The signature lapses after 1-2 hours; an expired one fails to load and the mark falls back.
/// HEIC is left out: `Image.network` cannot decode it on every platform.
final _signedLogoPath = RegExp(
  r'^/v1/files/[0-9a-f]{32}\.(png|jpe?g|webp)\?exp=\d+&sig=[0-9a-f]+$',
);

/// The current brand files are also served unsigned and immutable (`assets` in `/brand`); they never lapse.
final _publicFilePath = RegExp(r'^/v1/files/[0-9a-f]{32}\.(png|jpe?g|webp)$');

/// The business-profile fields the apps use. Only these are cached, never the whole profile.
const _brandKeys = [
  'name',
  'name_en',
  'short_name',
  'tagline',
  'tagline_en',
  'primary_color',
  'logo_url',
  'version',
];
const brandCacheKey = 'brand';

/// The brand for a public profile (`GET /v1/brand`). A missing or unusable value keeps the [fallback] value.
/// A colour that fails the white-label-text contrast check is ignored, so a bad choice never hides a button label.
/// Files count as own files on [apiBase] or on [publicBase] (the server's canonical address, after a domain move).
/// [accentOnly] (partner app): the fallback primary stays and the business colour becomes the accent.
BrandConfig brandFromProfile(
  Json profile, {
  required BrandConfig fallback,
  required String apiBase,
  String? publicBase,
  bool accentOnly = false,
}) {
  final colour = _brandColour(profile['primary_color']);
  final bases = [apiBase, ?publicBase];
  final assets = profile['assets'];
  final ownColour = colour != null && !accentOnly;
  return BrandConfig(
    name: _text(profile['name']) ?? fallback.name,
    nameEn: _text(profile['name_en']) ?? fallback.nameEn,
    shortName: _text(profile['short_name']) ?? fallback.shortName,
    tagline: _text(profile['tagline']) ?? fallback.tagline,
    taglineEn: _text(profile['tagline_en']) ?? fallback.taglineEn,
    version: _text(profile['version']),
    primary: ownColour ? colour : fallback.primary,
    primaryDeep: ownColour ? _shade(colour, -0.12) : fallback.primaryDeep,
    fresh: colour == null
        ? fallback.fresh
        : accentOnly
        ? colour
        : _shade(colour, 0.12),
    sun: fallback.sun,
    ink: fallback.ink,
    logoUrl:
        _ownFile(
          assets is Map ? assets['logo'] : null,
          bases,
          _publicFilePath,
        ) ??
        _ownFile(profile['logo_url'], bases, _signedLogoPath) ??
        fallback.logoUrl,
  );
}

/// Accepts only an upload URL on one of [bases]; anything else (foreign hosts, bare paths) is ignored.
String? _ownFile(Object? v, List<String> bases, RegExp path) {
  if (v is! String) return null;
  for (final base in bases) {
    if (v.startsWith(base) && path.hasMatch(v.substring(base.length))) {
      return v;
    }
  }
  return null;
}

String? _text(Object? v) {
  if (v is! String) return null;
  final t = v.trim();
  return t.isEmpty ? null : t;
}

Color? _brandColour(Object? v) {
  if (v is! String || !_hexColour.hasMatch(v)) return null;
  final c = Color(0xFF000000 | int.parse(v.substring(1), radix: 16));
  return whiteTextReadsOn(c) ? c : null;
}

Color _shade(Color c, double lightnessDelta) {
  final hsl = HSLColor.fromColor(c);
  return hsl
      .withLightness((hsl.lightness + lightnessDelta).clamp(0.0, 1.0))
      .toColor();
}

Map<String, Object?> _pick(Json profile) {
  final logo = profile['assets'] is Map ? profile['assets']['logo'] : null;
  return {
    for (final k in _brandKeys)
      if (profile[k] != null) k: profile[k],
    if (logo is String) 'assets': {'logo': logo},
  };
}

/// Holds the brand the apps paint with. Starts from the saved profile (or [fallback]) so the first frame
/// already has the right colours, then [refresh] pulls the live profile. Offline, the last good brand stays.
/// A new `X-Brand-Version` on any API answer fetches the profile again; see also [followResume].
class BrandController extends ChangeNotifier {
  BrandController({
    required this.api,
    required this.fallback,
    JsonCache? cache,
    this.accentOnly = false,
  }) : _cache = cache ?? JsonCache(),
       _brand = fallback {
    api.brandVersion.addListener(_onVersion);
    api.publicBase.addListener(_reapply);
  }

  final ApiClient api;
  final BrandConfig fallback;

  /// Partner app: keeps its own primary, takes the business colour as the accent.
  final bool accentOnly;
  final JsonCache _cache;
  BrandConfig _brand;
  Map<String, Object?> _saved = const {};
  Future<void>? _inFlight;

  /// The header value that last started a fetch: a server whose profile carries no version is asked once.
  String? _askedFor;

  BrandConfig get brand => _brand;

  /// Applies the saved profile, if any. A corrupt snapshot reads as no snapshot.
  Future<void> restore() async {
    final saved = await _cache.read(brandCacheKey);
    if (saved is Json) _apply(saved);
  }

  /// Fetches the public profile and saves its brand fields; parallel calls share one fetch.
  Future<void> refresh() =>
      _inFlight ??= _fetch().whenComplete(() => _inFlight = null);

  /// Refetches when the app comes back to the foreground. Dispose the result with the app.
  Foreground followResume({Duration every = const Duration(seconds: 30)}) =>
      Foreground(onResume: () => refresh().ignore(), every: every);

  /// An offline or failed call is not an error here: the brand is cosmetic, so the saved or built-in brand stays.
  Future<void> _fetch() async {
    final Object? body;
    try {
      body = await api.get('/brand');
    } on ApiException {
      return;
    }
    if (body is! Json) return;
    final saved = _pick(body);
    if (jsonEncode(saved) == jsonEncode(_saved)) return;
    await _cache.write(brandCacheKey, saved);
    _apply(saved);
  }

  void _onVersion() {
    final v = api.brandVersion.value;
    if (v == null || v == _saved['version'] || v == _askedFor) return;
    _askedFor = v;
    refresh().ignore();
  }

  void _reapply() {
    if (_saved.isNotEmpty) _apply(_saved);
  }

  void _apply(Json profile) {
    _saved = _pick(profile);
    _brand = brandFromProfile(
      profile,
      fallback: fallback,
      apiBase: api.baseUrl,
      publicBase: api.publicBase.value,
      accentOnly: accentOnly,
    );
    notifyListeners();
  }

  @override
  void dispose() {
    api.brandVersion.removeListener(_onVersion);
    api.publicBase.removeListener(_reapply);
    super.dispose();
  }
}

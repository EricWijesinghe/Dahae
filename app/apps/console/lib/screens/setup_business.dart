// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:typed_data';

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart';
import 'package:ui/ui.dart';

import '../brand_assets.dart';
import '../state.dart';
import 'brand_preview.dart';
import '../widgets/inline_button.dart';

/// Brand colours offered as one-tap swatches; any #RRGGBB can still be typed.
const _swatches = [
  '#187A8F',
  '#00B386',
  '#FF6B3D',
  '#8B5CF6',
  '#E11D74',
  '#1E9EBA',
  '#F59E0B',
  '#111827',
];

Color? parseHex(String s) {
  final v = s.trim().replaceFirst('#', '');
  if (v.length != 6) return null;
  final n = int.tryParse(v, radix: 16);
  return n == null ? null : Color(0xFF000000 | n);
}

/// Business profile: name, support contact, logo, brand colour and the legal footer details.
class BusinessCard extends StatefulWidget {
  const BusinessCard({super.key, required this.initial, required this.onSaved});
  final Json initial;
  final VoidCallback onSaved;

  @override
  State<BusinessCard> createState() => _BusinessCardState();
}

class _BusinessCardState extends State<BusinessCard> {
  static const _top = [
    'name',
    'name_en',
    'short_name',
    'tagline',
    'tagline_en',
    'support_phone',
    'support_email',
    'support_hours',
  ];
  static const _company = [
    'legal_name',
    'ceo',
    'registration_no',
    'mail_order_no',
    'address',
    'hosting_provider',
    'privacy_officer',
    'privacy_email',
  ];
  static const _links = [
    'app_store_url',
    'play_store_url',
    'partner_app_store_url',
    'partner_play_store_url',
    'kakao_channel_url',
    'naver_talk_url',
  ];
  // Search Console / Search Advisor tokens (meta tag content or the HTML-file name token).
  static const _seo = ['google_site_verification', 'naver_site_verification'];
  // Stored as app_versions.<app>.<field>; below min the app blocks, below latest it shows a banner.
  static const _versions = [
    'customer.min_app_version',
    'customer.latest_app_version',
    'partner.min_app_version',
    'partner.latest_app_version',
  ];

  late final Map<String, TextEditingController> _c;
  late final TextEditingController _color;
  String? _logo;
  // A picked file or a letter mark, not uploaded until Save.
  Uint8List? _logoBytes;
  String _logoFile = 'logo.png';
  late Json? _assets = widget.initial['brand_assets'] as Json?;
  String? _progress;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    final company = (widget.initial['company'] as Json?) ?? const {};
    _c = {
      for (final k in _top)
        k: TextEditingController(text: widget.initial[k] as String? ?? ''),
      for (final k in _company)
        'company.$k': TextEditingController(text: company[k] as String? ?? ''),
      for (final k in _links)
        k: TextEditingController(text: widget.initial[k] as String? ?? ''),
      for (final k in _seo)
        k: TextEditingController(text: widget.initial[k] as String? ?? ''),
      for (final k in _versions)
        k: TextEditingController(text: _savedVersion(k)),
    };
    _color = TextEditingController(
      text: widget.initial['primary_color'] as String? ?? _swatches.first,
    )..addListener(() => setState(() {}));
    _logo = widget.initial['logo_url'] as String?;
    _c['name']!.addListener(() => setState(() {}));
  }

  String _savedVersion(String key) {
    final app = key.split('.')[0], field = key.split('.')[1];
    final apps = widget.initial['app_versions'] as Json?;
    return (apps?[app] as Json?)?[field] as String? ?? '';
  }

  @override
  void dispose() {
    for (final c in _c.values) {
      c.dispose();
    }
    _color.dispose();
    super.dispose();
  }

  String _label(T t, String k) => switch (k) {
    'name' => t.cSetupBusinessName,
    'support_phone' => t.cSetupSupportPhone,
    'support_email' => t.cSetupSupportEmail,
    'support_hours' => t.cSetupSupportHours,
    'company.legal_name' => t.cSetupLegalName,
    'company.ceo' => t.cSetupCeo,
    'company.registration_no' => t.cSetupRegNo,
    'company.mail_order_no' => t.cSetupMailOrderNo,
    'company.address' => t.cSetupAddress,
    'company.hosting_provider' => t.cSetupHostingProvider,
    'company.privacy_email' => t.cRbPrivacyEmail,
    'name_en' => t.cRbNameEn,
    'short_name' => t.cRbShortName,
    'tagline' => t.cRbTagline,
    'tagline_en' => t.cRbTaglineEn,
    'app_store_url' => 'App Store · ${t.cSetupCustomerApp}',
    'play_store_url' => 'Google Play · ${t.cSetupCustomerApp}',
    'partner_app_store_url' => 'App Store · ${t.cSetupPartnerApp}',
    'partner_play_store_url' => 'Google Play · ${t.cSetupPartnerApp}',
    'kakao_channel_url' => '카카오 채널 URL (https://pf.kakao.com/...)',
    'naver_talk_url' => '네이버 톡톡 URL (https://talk.naver.com/...)',
    'google_site_verification' => 'Google Search Console 인증 코드',
    'naver_site_verification' => '네이버 서치어드바이저 인증 코드',
    'customer.min_app_version' => '${t.cSetupCustomerApp} · ${t.uMinVersion}',
    'customer.latest_app_version' =>
      '${t.cSetupCustomerApp} · ${t.uLatestVersion}',
    'partner.min_app_version' => '${t.cSetupPartnerApp} · ${t.uMinVersion}',
    'partner.latest_app_version' =>
      '${t.cSetupPartnerApp} · ${t.uLatestVersion}',
    _ => t.cSetupPrivacyOfficer,
  };

  Future<void> _pickLogo() async {
    final file = await ImagePicker().pickImage(
      source: ImageSource.gallery,
      maxWidth: 1024,
    );
    if (file == null) return;
    final bytes = await file.readAsBytes();
    if (!mounted) return;
    setState(() {
      _logoBytes = bytes;
      _logoFile = file.name;
    });
  }

  Future<void> _letterMark() async {
    final color = parseHex(_color.text) ?? BrandConfig.placeholder.primary;
    final bytes = await letterMark(_c['name']!.text, color);
    if (!mounted) return;
    setState(() {
      _logoBytes = bytes;
      _logoFile = 'logo.png';
    });
  }

  Json _body() => {
    for (final k in _top) k: _c[k]!.text,
    for (final k in _links) k: _c[k]!.text,
    for (final k in _seo) k: _c[k]!.text,
    'primary_color': _color.text,
    'logo_url': _logo,
    'brand_assets': _assets,
    'company': {for (final k in _company) k: _c['company.$k']!.text},
    'app_versions': {
      for (final app in ['customer', 'partner'])
        app: {
          'min_app_version': _c['$app.min_app_version']!.text,
          'latest_app_version': _c['$app.latest_app_version']!.text,
        },
    },
  };

  /// Draws the slot images from the new or the saved logo. Null when there is no logo to draw from.
  Future<Map<String, Uint8List>?> _derive() async {
    final bytes = _logoBytes, saved = _logo;
    if (bytes == null && saved == null) return null;
    final logo = bytes != null
        ? await decodeLogo(bytes)
        : await loadLogo(await _savedLogoUrl(saved!));
    return deriveBrandAssets(
      logo: logo,
      color: parseHex(_color.text) ?? BrandConfig.placeholder.primary,
      name: _c['name']!.text.trim(),
      tagline: _c['tagline']!.text.trim(),
    );
  }

  static const _maxLength = {
    'short_name': 12,
    'name_en': 60,
    'tagline': 60,
    'tagline_en': 60,
  };

  /// The saved logo's address, fresh from `/brand`: the one loaded with the form is signed and expires.
  Future<String> _savedLogoUrl(String saved) async {
    final api = context.api;
    final brand = await api.get('/brand');
    final current = brand is Json ? brand['logo_url'] as String? : null;
    return Uri.parse(
      api.baseUrl,
    ).resolve(freshLogoUrl(saved, current)).toString();
  }

  /// Logo first (it is the source), then the seven slots. Updates [_logo] and [_assets] as they land.
  Future<void> _upload(Map<String, Uint8List> files) async {
    final api = context.api, t = context.t;
    if (_logoBytes case final bytes?) {
      final url = await api.upload(bytes, _logoFile);
      setState(() {
        _logo = url;
        _logoBytes = null;
      });
    }
    final urls = await uploadBrandAssets(
      api,
      files,
      onProgress: (done, total) {
        if (mounted) {
          setState(() => _progress = t.cRbUploading('$done', '$total'));
        }
      },
    );
    setState(() => _assets = urls);
  }

  /// Logo -> images -> preview -> upload -> one PUT. The images are made again only when the logo,
  /// colour, name or tagline changed (or none exist yet).
  Future<void> _save() async {
    final t = context.t;
    final newLogo = _logoBytes != null;
    final remake = needsBrandRemake(widget.initial, {
      ..._body(),
      if (newLogo) 'logo_url': 'new',
    }, newLogo: newLogo);
    setState(() {
      _busy = true;
      _progress = remake ? t.cRbMaking : null;
    });
    try {
      if (remake) {
        final Map<String, Uint8List>? files;
        try {
          files = await _derive();
        } catch (_) {
          if (mounted) showToast(context, t.cRbDeriveFailed);
          return;
        }
        if (files == null || !mounted) return;
        if (!await confirmBrandPreview(context, files) || !mounted) return;
        await _upload(files);
      }
      await _put();
    } on ApiException catch (e) {
      if (mounted) {
        showToast(context, e.status == 400 ? t.cInvalidValue : t.error(e));
      }
    } catch (e) {
      if (mounted) showToast(context, t.error(e));
    } finally {
      if (mounted) {
        setState(() {
          _busy = false;
          _progress = null;
        });
      }
    }
  }

  Future<void> _put() async {
    await context.api.put('/admin/setup/business', _body());
    if (!mounted) return;
    showToast(context, context.t.cSetupSaved);
    context.consoleRead.brand?.refresh().ignore();
    await showBrandApplied(context);
    widget.onSaved();
  }

  /// The letter-mark offer, and a warning while a renamed business would still show the built-in mark.
  List<Widget> _logoHelp(T t) {
    if (_logo != null || _logoBytes != null) return const [];
    final name = _c['name']!.text.trim();
    return [
      if (name.isNotEmpty && name != BrandConfig.placeholder.name)
        Padding(
          padding: const EdgeInsets.only(top: Space.sm),
          child: Text(
            t.cRbNoLogoWarn,
            style: context.text.bodySmall?.copyWith(
              color: context.colors.danger,
            ),
          ),
        ),
      Align(
        alignment: Alignment.centerLeft,
        child: TextButton.icon(
          onPressed: _busy || name.isEmpty ? null : _letterMark,
          icon: const Icon(Icons.abc_rounded),
          label: Text(t.cRbLetterMark),
        ),
      ),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final brand = parseHex(_color.text) ?? context.primary;
    final done = (widget.initial['name'] as String?)?.isNotEmpty ?? false;
    Widget field(String k) => Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: TextField(
        controller: _c[k],
        maxLength: _maxLength[k], // the server's caps (brand profile)
        decoration: InputDecoration(labelText: _label(t, k)),
      ),
    );
    return AppCard(
      padding: EdgeInsets.zero,
      child: Theme(
        data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
        child: ExpansionTile(
          expandedCrossAxisAlignment: CrossAxisAlignment.stretch,
          initiallyExpanded: !done,
          tilePadding: const EdgeInsets.symmetric(
            horizontal: Space.lg,
            vertical: Space.xs,
          ),
          childrenPadding: const EdgeInsets.fromLTRB(
            Space.lg,
            0,
            Space.lg,
            Space.lg,
          ),
          leading: Icon(
            done ? Icons.check_circle_rounded : Icons.storefront_rounded,
            color: done ? context.primary : null,
          ),
          title: Text(t.cSetupBusiness, style: context.text.titleMedium),
          children: [
            _Preview(
              name: _c['name']!.text,
              color: brand,
              logo: _logo,
              logoBytes: _logoBytes,
              onPick: _busy ? null : _pickLogo,
              pickLabel: t.cSetupLogo,
            ),
            ..._logoHelp(t),
            const SizedBox(height: Space.md),
            for (final k in _top) field(k),
            Text(t.cSetupColor, style: context.text.labelLarge),
            const SizedBox(height: Space.xs),
            Wrap(
              spacing: Space.xs,
              runSpacing: Space.xs,
              children: [
                for (final s in _swatches)
                  Semantics(
                    button: true,
                    label: s,
                    child: InkResponse(
                      onTap: () => _color.text = s,
                      radius: 28,
                      child: Container(
                        width: 48,
                        height: 48,
                        decoration: BoxDecoration(
                          color: parseHex(s),
                          shape: BoxShape.circle,
                          border: Border.all(
                            color: _color.text.toUpperCase() == s
                                ? context.colors.ink
                                : Colors.transparent,
                            width: 3,
                          ),
                        ),
                      ),
                    ),
                  ),
                SizedBox(
                  width: 130,
                  child: TextField(
                    controller: _color,
                    decoration: const InputDecoration(
                      labelText: '#RRGGBB',
                      isDense: true,
                    ),
                  ),
                ),
              ],
            ),
            // Customers and partners see white text on this colour (buttons): flag a colour they cannot read.
            if (parseHex(_color.text) case final c? when !whiteTextReadsOn(c))
              Padding(
                padding: const EdgeInsets.only(top: Space.xs),
                child: Text(
                  t.cSetupColorContrast,
                  style: context.text.bodySmall?.copyWith(
                    color: context.colors.danger,
                  ),
                ),
              ),
            const SizedBox(height: Space.lg),
            Text(t.cSetupCompany, style: context.text.labelLarge),
            const SizedBox(height: Space.xs),
            for (final k in _company) field('company.$k'),
            const SizedBox(height: Space.md),
            Text(t.cSetupStoreLinks, style: context.text.labelLarge),
            const SizedBox(height: Space.xs),
            for (final k in _links) field(k),
            const SizedBox(height: Space.md),
            Text('검색·문의 (Search & chat)', style: context.text.labelLarge),
            const SizedBox(height: Space.xs),
            for (final k in _seo) field(k),
            const SizedBox(height: Space.md),
            Text(t.uAppVersionTitle, style: context.text.labelLarge),
            const SizedBox(height: Space.xs),
            Text(t.uAppVersionHint, style: context.text.bodySmall),
            const SizedBox(height: Space.xs),
            for (final k in _versions) field(k),
            const SizedBox(height: Space.sm),
            Align(
              alignment: Alignment.centerLeft,
              child: FilledButton.icon(
                style: inlineFilled,
                onPressed: _busy ? null : _save,
                icon: const Icon(Icons.save_rounded),
                label: Text(t.cSetupSave),
              ),
            ),
            if (_progress case final p?)
              Padding(
                padding: const EdgeInsets.only(top: Space.xs),
                child: Text(p, style: context.text.bodySmall),
              ),
          ],
        ),
      ),
    );
  }
}

/// Live preview of the app header with the chosen name, logo and colour.
class _Preview extends StatelessWidget {
  const _Preview({
    required this.name,
    required this.color,
    required this.logo,
    required this.logoBytes,
    required this.onPick,
    required this.pickLabel,
  });
  final String name;
  final Color color;
  final String? logo;
  final Uint8List? logoBytes;
  final VoidCallback? onPick;
  final String pickLabel;

  @override
  Widget build(BuildContext context) {
    final onColor = color.computeLuminance() > 0.5
        ? Colors.black
        : Colors.white;
    return Container(
      padding: const EdgeInsets.all(Space.lg),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(Radii.card),
        gradient: LinearGradient(
          colors: [color, Color.lerp(color, Colors.black, 0.35)!],
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
        ),
        boxShadow: [
          BoxShadow(
            color: color.withValues(alpha: 0.35),
            blurRadius: 24,
            offset: const Offset(0, 12),
          ),
        ],
      ),
      child: Row(
        children: [
          Semantics(
            button: true,
            label: pickLabel,
            child: InkWell(
              onTap: onPick,
              borderRadius: BorderRadius.circular(16),
              child: Container(
                width: 64,
                height: 64,
                decoration: BoxDecoration(
                  color: onColor.withValues(alpha: 0.15),
                  borderRadius: BorderRadius.circular(16),
                ),
                clipBehavior: Clip.antiAlias,
                child: logoBytes != null
                    ? Image.memory(logoBytes!, fit: BoxFit.cover)
                    : logo == null
                    ? Icon(Icons.add_photo_alternate_rounded, color: onColor)
                    : Image.network(
                        logo!,
                        fit: BoxFit.cover,
                        // 64dp box, 3x screens.
                        cacheWidth: 192,
                        cacheHeight: 192,
                        errorBuilder: (_, _, _) =>
                            Icon(Icons.broken_image_outlined, color: onColor),
                      ),
              ),
            ),
          ),
          const SizedBox(width: Space.md),
          Expanded(
            child: Text(
              name.isEmpty ? BrandConfig.placeholder.name : name,
              style: context.text.headlineSmall?.copyWith(
                color: onColor,
                fontWeight: FontWeight.w800,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

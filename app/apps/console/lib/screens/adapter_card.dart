// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';
import 'package:url_launcher/url_launcher.dart';

import '../state.dart';
import '../widgets/copy_row.dart';
import '../widgets/inline_button.dart';
import 'setup_guides.dart';
import 'setup_owner_code.dart';

/// The setup catalog's adapters that have no card of their own, each on a generic [AdapterCard]. Sections with a
/// bespoke card (every Korean service today) and adapters without keys show nothing here. A server older than the
/// catalog answers 404, which also shows nothing.
class AdapterCatalog extends StatefulWidget {
  const AdapterCatalog({super.key, required this.onChanged});
  final VoidCallback onChanged;

  @override
  State<AdapterCatalog> createState() => _AdapterCatalogState();
}

class _AdapterCatalogState extends State<AdapterCatalog> {
  late Future<List<Json>> _entries = _load();

  Future<List<Json>> _load() async {
    final Json data;
    try {
      data = await context.api.get('/admin/setup/catalog') as Json;
    } on ApiException catch (e) {
      if (e.status == 404) return const [];
      rethrow;
    }
    return [
      for (final s in (data['sections'] as List).cast<Json>())
        for (final a in (s['adapters'] as List).cast<Json>())
          if (genericCardFor(a)) a,
    ];
  }

  void _changed() {
    setState(() => _entries = _load());
    widget.onChanged();
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<List<Json>>(
    future: _entries,
    builder: (context, snap) {
      if (snap.hasError) {
        return Padding(
          padding: const EdgeInsets.only(top: Space.md),
          child: KeepAllText(context.t.error(snap.error!)),
        );
      }
      final entries = snap.data ?? const <Json>[];
      if (entries.isEmpty) return const SizedBox.shrink();
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const SizedBox(height: Space.md),
          KeepAllText(
            context.t.cAdapterOtherServices,
            style: context.text.titleMedium,
          ),
          for (final a in entries) ...[
            const SizedBox(height: Space.sm),
            AdapterCard(key: ValueKey(a['id']), entry: a, onChanged: _changed),
          ],
        ],
      );
    },
  );
}

/// An adapter gets the generic card when it has keys to enter and its section has no bespoke card.
bool genericCardFor(Json a) =>
    (a['fields'] as List).isNotEmpty && !guides.containsKey(a['section']);

/// One adapter from the setup catalog: docs links, webhook URL, its fields, save / check / unlock / disconnect.
/// Stored values never reach it: a field shows only whether it is set, and typed secrets leave the box on save.
class AdapterCard extends StatefulWidget {
  const AdapterCard({super.key, required this.entry, required this.onChanged});
  final Json entry;
  final VoidCallback onChanged;

  @override
  State<AdapterCard> createState() => _AdapterCardState();
}

class _AdapterCardState extends State<AdapterCard> {
  late final Map<String, TextEditingController> _text = {
    for (final f in _fields)
      if (f['kind'] != 'toggle') f['name'] as String: TextEditingController(),
  };
  // Null until touched: an untouched switch is not sent, so the saved or default position stays.
  final _toggles = <String, bool>{};
  final _revealed = <String>{};
  bool _busy = false;
  Json? _check;

  List<Json> get _fields => (widget.entry['fields'] as List).cast<Json>();
  String get _section => widget.entry['section'] as String;
  bool get _locked => widget.entry['locked'] == true;
  bool get _saved => widget.entry['source'] == 'console';

  @override
  void dispose() {
    for (final c in _text.values) {
      c.dispose();
    }
    super.dispose();
  }

  Json _body() => {
    for (final e in _text.entries)
      if (e.value.text.trim().isNotEmpty) e.key: e.value.text.trim(),
    ..._toggles,
  };

  Future<void> _run(Future<void> Function() action) async {
    setState(() => _busy = true);
    try {
      await action();
    } catch (e) {
      if (mounted) showToast(context, _errorText(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _errorText(Object e) {
    final t = context.t;
    if (e is! ApiException) return t.error(e);
    if (e.code == 'totp_invalid') return t.cOwnerCodeWrong;
    return switch (e.status) {
      400 => setupMessage(t, e.code),
      409 when e.code == 'card_locked' => t.cSetupLocked,
      403 => t.cSetupOwnerOnly,
      429 => t.cOwnerCodeLocked,
      _ => t.error(e),
    };
  }

  /// The owner's code when the server asks for it on this save; null when the owner cancelled.
  Future<Map<String, String>?> _ownerHeaders() async {
    if (widget.entry['owner_code'] != true) return const {};
    final code = await askOwnerCode(context);
    return code == null ? null : {ownerTotpHeader: code};
  }

  Future<void> _save() => _run(() async {
    final headers = await _ownerHeaders();
    if (headers == null || !mounted) return;
    await context.api.put('/admin/setup/$_section', _body(), headers);
    if (!mounted) return;
    setState(() {
      for (final c in _text.values) {
        c.clear();
      }
      _toggles.clear();
      _revealed.clear();
      _check = null;
    });
    showToast(context, context.t.cSetupSaved);
    widget.onChanged();
  });

  Future<void> _test() => _run(() async {
    final r = await context.api.post('/admin/setup/$_section/test') as Json;
    if (mounted) setState(() => _check = r);
  });

  Future<void> _unlock() => _run(() async {
    final code = await askOwnerCode(context);
    if (code == null || !mounted) return;
    await context.api.post('/admin/setup/$_section/unlock', null, null, {
      ownerTotpHeader: code,
    });
    widget.onChanged();
  });

  Future<void> _clear() => _run(() async {
    await context.api.delete('/admin/setup/$_section');
    widget.onChanged();
  });

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final e = widget.entry;
    final configured = e['configured'] == true;
    final docs = (e['docs'] as Json?) ?? const {};
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KeepAllText(e['name'] as String, style: context.text.titleMedium),
          const SizedBox(height: Space.xs),
          Wrap(
            spacing: Space.xs,
            runSpacing: Space.xs,
            children: [
              configured
                  ? Tag(label: t.cSetupConnected, hue: StatusHues.confirmed)
                  : Tag(
                      label: t.cSetupNotConnected,
                      hue: StatusHues.consultation,
                    ),
              if (e['recommended'] == true)
                Tag(label: t.cAdapterRecommended, hue: StatusHues.assigned),
              if (_locked)
                Tag(label: t.cSetupLockedBadge, hue: StatusHues.cancelled),
              if (e['mode'] case final String mode)
                Tag(
                  label: mode == 'live' ? t.cSetupModeLive : t.cSetupModeTest,
                  hue: mode == 'live'
                      ? StatusHues.assigned
                      : StatusHues.consultation,
                ),
              if (_check case final Json c)
                Tag(
                  label: setupMessage(t, c['code'] as String),
                  hue: c['ok'] == true
                      ? StatusHues.assigned
                      : StatusHues.cancelled,
                ),
            ],
          ),
          Wrap(
            spacing: Space.sm,
            children: [
              if (docs['keys_url'] case final String url)
                _link(url, t.cAdapterKeysLink),
              if (docs['signup_url'] case final String url)
                _link(url, t.cAdapterSignupLink),
            ],
          ),
          if (e['webhook_url'] case final String url)
            CopyRow(label: t.cAdapterWebhook, value: url),
          const SizedBox(height: Space.sm),
          for (final f in _fields) _field(f),
          const SizedBox(height: Space.sm),
          Wrap(
            spacing: Space.sm,
            runSpacing: Space.sm,
            children: [
              FilledButton.icon(
                style: inlineFilled,
                onPressed: _busy || _locked ? null : _save,
                icon: const Icon(Icons.save_rounded),
                label: KeepAllText(t.cSetupSave),
              ),
              if (configured)
                OutlinedButton.icon(
                  onPressed: _busy ? null : _test,
                  icon: const Icon(Icons.wifi_tethering_rounded),
                  label: KeepAllText(t.cSetupTest),
                ),
              if (_locked && _saved)
                OutlinedButton.icon(
                  onPressed: _busy ? null : _unlock,
                  icon: const Icon(Icons.lock_open_rounded),
                  label: KeepAllText(t.cSetupUnlock),
                ),
              if (_saved && !_locked)
                TextButton(
                  onPressed: _busy ? null : _clear,
                  child: KeepAllText(t.cSetupClear),
                ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _link(String url, String label) => TextButton.icon(
    onPressed: () =>
        launchUrl(Uri.parse(url), mode: LaunchMode.externalApplication),
    icon: const Icon(Icons.open_in_new_rounded, size: 18),
    label: KeepAllText(label),
  );

  /// `client_secret` reads as "Client secret": catalog fields carry no translated label.
  String _label(Json f) {
    final words = (f['name'] as String).replaceAll('_', ' ');
    final name = words.isEmpty
        ? words
        : words[0].toUpperCase() + words.substring(1);
    return f['required'] == true ? name : '$name (${context.t.cSetupOptional})';
  }

  Widget _field(Json f) {
    final t = context.t;
    final name = f['name'] as String;
    final kind = f['kind'] as String;
    if (kind == 'toggle') {
      return SwitchListTile(
        contentPadding: EdgeInsets.zero,
        title: KeepAllText(_label(f)),
        value: _toggles[name] ?? f['value'] == true,
        onChanged: _locked ? null : (on) => setState(() => _toggles[name] = on),
      );
    }
    // Pasted files are multi-line, which a masked box cannot be; like secrets, they are never read back.
    final secret = kind == 'secret';
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: TextField(
        controller: _text[name],
        enabled: !_locked,
        obscureText: secret && !_revealed.contains(name),
        maxLines: kind == 'file' || kind == 'map' ? 6 : 1,
        minLines: 1,
        autocorrect: false,
        enableSuggestions: false,
        decoration: InputDecoration(
          labelText: _label(f),
          helperText: f['set'] == true ? t.cSetupKeepValue : t.cAdapterNotSet,
          suffixIcon: secret
              ? IconButton(
                  tooltip: _revealed.contains(name)
                      ? t.cSetupHide
                      : t.cSetupShow,
                  icon: Icon(
                    _revealed.contains(name)
                        ? Icons.visibility_off_rounded
                        : Icons.visibility_rounded,
                  ),
                  onPressed: () => setState(
                    () => _revealed.contains(name)
                        ? _revealed.remove(name)
                        : _revealed.add(name),
                  ),
                )
              : null,
        ),
      ),
    );
  }
}

// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:math' as math;

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import 'setup_guides.dart';
import 'setup_language.dart';

/// Setup steps 0 and 1 on a fresh install: the console language, then the country, with a preview of what its pack
/// sets up. `GET /admin/setup/packs` lists the packs (`{country, detected, packs: [pack]}`); the choice goes to
/// `PUT /admin/setup/country`. A server without the list (404) or an install whose country is set (every existing
/// Korean install) shows nothing, so the steps never reach an install that already runs.
class SetupCountryCard extends StatefulWidget {
  const SetupCountryCard({super.key, required this.onSaved});
  final VoidCallback onSaved;

  @override
  State<SetupCountryCard> createState() => _SetupCountryCardState();
}

class _SetupCountryCardState extends State<SetupCountryCard> {
  Json? _data;
  Object? _error;
  String? _picked;
  String _query = '';
  bool _ack = false;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  List<Json> get _packs => _packsOf(_data ?? const {});

  Json? get _pack => _packs.where((p) => p['id'] == _picked).firstOrNull;

  Future<void> _load() async {
    setState(() => _error = null);
    try {
      final data = await context.api.get('/admin/setup/packs') as Json;
      if (!mounted) return;
      final listed = _packsOf(data).map((p) => p['id']).toSet();
      setState(() {
        _data = data;
        // The detected region is only a default: nothing is saved until the owner confirms.
        _picked ??= listed.contains(data['detected'])
            ? data['detected'] as String
            : null;
      });
    } on ApiException catch (e) {
      if (mounted && e.status != 404) setState(() => _error = e);
    }
  }

  static List<Json> _packsOf(Json data) => switch (data['packs']) {
    final List<Object?> list => list.whereType<Json>().toList(),
    _ => const [],
  };

  Future<void> _confirm(Json pack) async {
    setState(() => _busy = true);
    try {
      await context.api.put('/admin/setup/country', {
        'country': pack['id'],
        if (packIsDraft(pack)) 'draft_ack': true,
      });
      if (!mounted) return;
      showToast(context, context.t.cCountrySaved);
      setState(() => _data = {...?_data, 'country': pack['id']});
      widget.onSaved();
    } on ApiException catch (e) {
      if (mounted) showToast(context, context.t.error(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    if (_error case final ApiException e) {
      return ErrorRetry(
        message: t.error(e),
        retryLabel: t.retry,
        onRetry: _load,
      );
    }
    final data = _data;
    if (data == null || data['country'] != null || _packs.isEmpty) {
      return const SizedBox.shrink();
    }
    final pack = _pack;
    final ready = pack != null && !_busy && (!packIsDraft(pack) || _ack);
    return Padding(
      padding: const EdgeInsetsDirectional.only(bottom: Space.md),
      child: AppCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const SetupLanguageStep(),
            const Divider(height: Space.xl),
            KeepAllText(t.cCountryStepTitle, style: context.text.titleMedium),
            const SizedBox(height: Space.xs),
            KeepAllText(t.cCountryStepBody, style: context.text.bodyMedium),
            if (_detectedName(data) case final name?) ...[
              const SizedBox(height: Space.xs),
              KeepAllText(
                t.cCountryDetected(name),
                style: context.text.bodySmall,
              ),
            ],
            const SizedBox(height: Space.sm),
            TextField(
              decoration: InputDecoration(
                prefixIcon: const Icon(Icons.search_rounded),
                hintText: t.cCountrySearch,
              ),
              onChanged: (v) => setState(() => _query = v.trim().toLowerCase()),
            ),
            const SizedBox(height: Space.xs),
            ..._list(t),
            if (pack != null) ...[
              const SizedBox(height: Space.md),
              CountryPreviewCard(pack: pack),
              if (packIsDraft(pack))
                CheckboxListTile(
                  contentPadding: EdgeInsets.zero,
                  controlAffinity: ListTileControlAffinity.leading,
                  value: _ack,
                  onChanged: (v) => setState(() => _ack = v ?? false),
                  title: KeepAllText(t.cCountryDraftAck),
                ),
              const SizedBox(height: Space.sm),
              FilledButton(
                onPressed: ready ? () => _confirm(pack) : null,
                child: Text(t.cCountryConfirm),
              ),
              const SizedBox(height: Space.xs),
              KeepAllText(t.cCountryChangeNote, style: context.text.bodySmall),
            ],
          ],
        ),
      ),
    );
  }

  String? _detectedName(Json data) => _packs
      .where((p) => p['id'] == data['detected'])
      .map((p) => p['name'] as String?)
      .firstOrNull;

  List<Widget> _list(T t) {
    final shown = [
      for (final p in _packs)
        if (_query.isEmpty ||
            '${p['name']} ${p['id']}'.toLowerCase().contains(_query))
          p,
    ];
    if (shown.isEmpty) {
      return [KeepAllText(t.cCountryNone, style: context.text.bodyMedium)];
    }
    return [
      for (final p in shown)
        ListTile(
          contentPadding: EdgeInsets.zero,
          selected: p['id'] == _picked,
          leading: Icon(
            p['id'] == _picked
                ? Icons.radio_button_checked_rounded
                : Icons.radio_button_unchecked_rounded,
          ),
          title: Text('${p['name'] ?? p['id']}'),
          trailing: p['contact_only'] == true
              ? Text(t.cCountryContact)
              : MaturityBadge(pack: p),
          enabled: p['contact_only'] != true,
          onTap: () => setState(() {
            _picked = p['id'] as String?;
            _ack = false;
          }),
        ),
    ];
  }
}

bool packIsDraft(Json pack) => switch (pack['maturity']) {
  {'level': 'reviewed'} => false,
  _ => true,
};

/// "Reviewed" or "Draft": whether a lawyer and an accountant have checked the pack.
class MaturityBadge extends StatelessWidget {
  const MaturityBadge({super.key, required this.pack});
  final Json pack;

  @override
  Widget build(BuildContext context) {
    final draft = packIsDraft(pack);
    final c = context.colors;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: Space.sm, vertical: 2),
      decoration: BoxDecoration(
        color: draft ? c.sunTint : c.freshTint,
        borderRadius: BorderRadius.circular(Space.sm),
      ),
      child: Text(
        draft ? context.t.cCountryDraft : context.t.cCountryReviewed,
        style: context.text.labelSmall?.copyWith(color: c.ink),
      ),
    );
  }
}

/// What choosing [pack] sets up: currency (with a sample price), time zones, customer languages, recommended
/// services, the legal documents that will be prepared, and how far the pack has been reviewed.
class CountryPreviewCard extends StatelessWidget {
  const CountryPreviewCard({super.key, required this.pack});
  final Json pack;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final lang = context.lang;
    final currency = Currency.fromJson(pack['currency']);
    final rows = <(String, String)>[
      if (currency != null)
        (
          t.cCountryCurrency,
          '${currency.code} · ${t.cCountryPriceSample(formatMoney(samplePrice(currency), currency, lang))}',
        ),
      (t.cCountryZones, _strings(pack['zones']).join(', ')),
      (
        t.cCountryLanguages,
        [
          for (final c in _strings(pack['locales']))
            LocaleController.names[c] ?? c,
        ].join(', '),
      ),
      (
        t.cCountryProviders,
        {
          for (final id in [
            ..._strings((pack['payments'] as Json?)?['recommended']),
            ..._strings((pack['messaging'] as Json?)?['recommended']),
          ])
            guides[id]?.title(t) ?? id,
        }.join(', '),
      ),
      (
        t.cCountryLegal,
        [for (final d in _strings(pack['legal'])) _doc(t, d)].join(', '),
      ),
    ];
    return Container(
      padding: const EdgeInsets.all(Space.md),
      decoration: BoxDecoration(
        color: context.colors.surfaceHigh,
        borderRadius: BorderRadius.circular(Space.md),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KeepAllText(
            t.cCountryPreviewTitle('${pack['name'] ?? pack['id']}'),
            style: context.text.titleSmall,
          ),
          const SizedBox(height: Space.sm),
          for (final (label, value) in rows)
            if (value.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(bottom: Space.xs),
                child: KeepAllText(
                  '$label: $value',
                  style: context.text.bodyMedium,
                ),
              ),
          Wrap(
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text('${t.cCountryMaturity}: ', style: context.text.bodyMedium),
              MaturityBadge(pack: pack),
            ],
          ),
          if (packIsDraft(pack)) ...[
            const SizedBox(height: Space.sm),
            KeepAllText(
              t.cCountryDraftWarn,
              style: context.text.bodyMedium?.copyWith(
                color: context.colors.danger,
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// A price as customers would see it: 49,000 in a currency without decimals, 49.90 in one with two.
int samplePrice(Currency c) => c.minorUnits == 0
    ? 49000
    : 4990 * math.pow(10, c.minorUnits).toInt() ~/ 100;

List<String> _strings(Object? v) => switch (v) {
  final List<Object?> list => list.whereType<String>().toList(),
  _ => const [],
};

String _doc(T t, String doc) => switch (doc) {
  'terms' => t.cCountryDocTerms,
  'privacy' => t.cCountryDocPrivacy,
  'location_terms' => t.cCountryDocLocation,
  _ => doc,
};

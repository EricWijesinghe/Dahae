// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:async';

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';
import 'package:url_launcher/url_launcher.dart';

import '../state.dart';
import '../widgets/copy_row.dart';
import 'host_controls.dart';
import 'setup_app_links.dart';
import 'setup_business.dart';
import 'setup_domain.dart';
import 'setup_formats.dart';
import 'setup_guides.dart';
import 'setup_operator.dart';
import 'setup_owner_code.dart';
import 'setup_pay_methods.dart';
import 'setup_review.dart';
import 'setup_store_build.dart';
import '../widgets/inline_button.dart';

/// Owner's launch checklist: business profile plus one card per external service.
/// Keys are typed here (never in config files), saved encrypted, and tested on the spot.
class SetupScreen extends StatefulWidget {
  const SetupScreen({super.key});
  @override
  State<SetupScreen> createState() => _SetupScreenState();
}

class _SetupScreenState extends State<SetupScreen> {
  final _view = GlobalKey<AsyncViewState<Json>>();

  void _reload() => _view.currentState?.reload();

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return AsyncView<Json>(
      key: _view,
      load: () async => await context.api.get('/admin/setup') as Json,
      builder: (context, data, reload) {
        final cards = (data['integrations'] as List).cast<Json>();
        final business = (data['business'] as Json?) ?? const {};
        // PortOne and Toss Payments are one item: either one is enough to take payments.
        final others = cards.where((c) => !_payCards.contains(c['code']));
        final pay = cards.where((c) => _payCards.contains(c['code']));
        final done =
            others.where((c) => c['configured'] == true).length +
            (pay.any((c) => c['configured'] == true) ? 1 : 0) +
            (business['name'] != null ? 1 : 0);
        final total = others.length + (pay.isEmpty ? 0 : 1) + 1;
        return Align(
          alignment: Alignment.topCenter,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 920),
            child: ListView(
              padding: const EdgeInsets.all(Space.lg),
              children: [
                _Header(
                  done: done,
                  total: total,
                  devMode: data['dev_mode'] == true,
                ),
                const SizedBox(height: Space.lg),
                BusinessCard(initial: business, onSaved: _reload),
                if (data['store_build'] case final Json build) ...[
                  const SizedBox(height: Space.md),
                  StoreBuildCard(data: build, onAcked: _reload),
                ],
                const SizedBox(height: Space.md),
                const BusinessHoursCard(),
                const SizedBox(height: Space.md),
                const PayMethodsCard(),
                const SizedBox(height: Space.md),
                ReviewAccountCard(
                  current: data['review_account'] as Json?,
                  onChanged: _reload,
                ),
                const SizedBox(height: Space.md),
                HeartbeatCard(configured: data['heartbeat_configured'] == true),
                const SizedBox(height: Space.md),
                const AppLinksCard(),
                const SizedBox(height: Space.md),
                DomainCard(business: business),
                // A domain move needs the server approval code, so it can be made here too.
                const SizedBox(height: Space.md),
                const AppCard(child: ApprovalEnrollCard()),
                for (final c in cards) ...[
                  const SizedBox(height: Space.md),
                  if (c['code'] == 'portone') ...[
                    KeepAllText(t.cPaymentsTitle, style: context.text.titleMedium),
                    if (_gatewayLine(t, data, context.lang) case final line?)
                      KeepAllText(line, style: context.text.bodyMedium),
                    const SizedBox(height: Space.sm),
                  ],
                  IntegrationCard(
                    key: ValueKey(c['code']),
                    card: c,
                    webhookUrl: data['webhook_url'] as String?,
                    tossWebhookUrl: data['toss_webhook_url'] as String?,
                    tossPaymentsWebhookUrl:
                        data['toss_payments_webhook_url'] as String?,
                    onChanged: _reload,
                  ),
                ],
                const SizedBox(height: Space.md),
                // `env`: TOSS_PAYOUTS_ENABLED on the server turns payouts on whatever the saved switch says.
                TossPayoutSwitchCard(
                  on: switch (data['payout_toss_switch']) {
                    {'on': true} || {'env': true} => true,
                    _ => false,
                  },
                ),
                const SizedBox(height: Space.xl),
                KeepAllText(
                  t.cSetupProgress('$done', '$total'),
                  textAlign: TextAlign.center,
                  style: context.text.bodySmall,
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

const _payCards = {'portone', 'tosspayments'};

/// Every gateway a new payment can reach, PortOne first, each marked when it runs on test keys.
/// A server older than `payment_gateways` sends only its one gateway; one older still (v1.5.1) sends none.
String? _gatewayLine(T t, Json data, String lang) {
  final gateways = switch (data['payment_gateways']) {
    final List<Object?> list => list.cast<Json>(),
    _ when data['payment_gateway'] != null => [
      {'code': data['payment_gateway'], 'mode': data['payment_gateway_mode']},
    ],
    _ => null,
  };
  if (gateways == null) return null;
  final names = [
    for (final g in gateways)
      if (gatewayNames[g['code']] case final name?)
        [
          name.of(lang),
          if (g['mode'] == 'test') t.cSetupGatewayTestKeys,
        ].join(' '),
  ];
  return names.isEmpty
      ? t.cSetupGatewayNone
      : t.cSetupGatewayLive(names.join(', '));
}

class _Header extends StatelessWidget {
  const _Header({
    required this.done,
    required this.total,
    required this.devMode,
  });
  final int done;
  final int total;
  final bool devMode;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final finished = done == total;
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                finished
                    ? Icons.rocket_launch_rounded
                    : Icons.checklist_rounded,
                color: context.primary,
                size: 28,
              ),
              const SizedBox(width: Space.sm),
              Expanded(
                child: KeepAllText(
                  finished ? t.cSetupAllDone : t.cSetupTitle,
                  style: context.text.titleLarge,
                ),
              ),
              Flexible(
                child: KeepAllText(
                  t.cSetupProgress('$done', '$total'),
                  textAlign: TextAlign.end,
                  style: context.text.titleSmall,
                ),
              ),
            ],
          ),
          const SizedBox(height: Space.md),
          ClipRRect(
            borderRadius: BorderRadius.circular(8),
            child: TweenAnimationBuilder<double>(
              tween: Tween(begin: 0, end: total == 0 ? 0 : done / total),
              duration: const Duration(milliseconds: 600),
              curve: Motion.ease,
              builder: (_, v, _) =>
                  LinearProgressIndicator(value: v, minHeight: 10),
            ),
          ),
          if (devMode) ...[
            const SizedBox(height: Space.md),
            Row(
              children: [
                const Icon(Icons.science_rounded, size: 18),
                const SizedBox(width: Space.xs),
                Expanded(
                  child: KeepAllText(t.cSetupDevMode, style: context.text.bodySmall),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

/// One external service: status, the how-to steps, the fields, save / test / disconnect.
class IntegrationCard extends StatefulWidget {
  const IntegrationCard({
    super.key,
    required this.card,
    required this.onChanged,
    this.webhookUrl,
    this.tossWebhookUrl,
    this.tossPaymentsWebhookUrl,
  });
  final Json card;
  final VoidCallback onChanged;
  final String? webhookUrl;
  final String? tossWebhookUrl;
  final String? tossPaymentsWebhookUrl;

  @override
  State<IntegrationCard> createState() => _IntegrationCardState();
}

class _IntegrationCardState extends State<IntegrationCard> {
  final _text = <String, TextEditingController>{};
  final _map = <String, Map<String, TextEditingController>>{};
  // Null until the owner touches a switch or it is saved; an untouched switch is not sent (env default stays).
  final _toggles = <String, bool?>{};
  // Secret fields the owner has asked to see while typing (show/hide).
  final _revealed = <String>{};
  // Saved map slots (channel keys) the owner asked to remove: sent as null, which the server deletes.
  final _clearedSlots = <String>{};
  bool _busy = false;
  // The latest connection check, passed or not.
  Json? _result;
  // The stored passing check: a failed payout check does not replace it, and it keeps Toss open.
  Json? _lastPass;
  // Keeps the unlock countdown current and reloads once the server has closed the window.
  Timer? _lockTimer;

  String get _code => widget.card['code'] as String;
  // Payment gateway cards: owner code on first save, "API 키 확인" as the check label.
  bool get _gatewayCard => _code == 'portone' || _code == 'tosspayments';
  // The server locks a saved card; the screen only reflects it.
  bool get _locked => widget.card['locked'] == true;
  Guide get _guide => guides[_code]!;
  List<Json> get _fields => (widget.card['fields'] as List).cast<Json>();
  DateTime? get _unlockUntil {
    final raw = widget.card['unlock_expires_at'];
    return raw is String ? DateTime.tryParse(raw)?.toLocal() : null;
  }

  @override
  void didUpdateWidget(IntegrationCard old) {
    super.didUpdateWidget(old);
    _watchUnlock();
  }

  void _watchUnlock() {
    _lockTimer?.cancel();
    _lockTimer = _unlockUntil == null
        ? null
        : Timer.periodic(const Duration(seconds: 15), (_) => _tickUnlock());
  }

  void _tickUnlock() {
    final until = _unlockUntil;
    if (!mounted || until == null) return;
    if (DateTime.now().isBefore(until)) {
      setState(() {});
      return;
    }
    _lockTimer?.cancel();
    widget.onChanged();
  }

  String _unlockBadge(T t) {
    final until = _unlockUntil;
    if (until == null) return t.cSetupUnlockedBadge;
    final minutes = (until.difference(DateTime.now()).inSeconds / 60).ceil();
    return t.cSetupUnlockLeft('${minutes < 1 ? 1 : minutes}');
  }

  @override
  void initState() {
    super.initState();
    _watchUnlock();
    _result = (widget.card['last_check'] ?? widget.card['last_test']) as Json?;
    _lastPass = widget.card['last_test'] as Json?;
    for (final f in _fields) {
      final name = f['name'] as String;
      final kind = f['kind'] as String;
      final preview = f['preview'];
      if (kind == 'map') {
        final saved = preview is Map
            ? preview.cast<String, dynamic>()
            : const <String, dynamic>{};
        final keys = {
          ..._guide.fields[name]?.suggestions ?? const <String>[],
          ...saved.keys,
        };
        _map[name] = {for (final k in keys) k: TextEditingController()};
      } else if (kind == 'toggle') {
        _toggles[name] = f['set'] == true ? preview == true : null;
      } else {
        // Saved values are never read back into the box: the server only returns a masked tail.
        _text[name] = TextEditingController();
      }
    }
  }

  @override
  void dispose() {
    _lockTimer?.cancel();
    for (final c in _text.values) {
      c.dispose();
    }
    for (final m in _map.values) {
      for (final c in m.values) {
        c.dispose();
      }
    }
    super.dispose();
  }

  Json _body() => {
    for (final e in _text.entries)
      if (e.value.text.trim().isNotEmpty) e.key: e.value.text.trim(),
    for (final e in _toggles.entries)
      if (e.value != null) e.key: e.value,
    for (final e in _map.entries)
      e.key: {
        for (final m in e.value.entries)
          if (m.value.text.trim().isNotEmpty)
            m.key: m.value.text.trim()
          else if (_clearedSlots.contains(m.key))
            m.key: null,
      },
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

  /// A wrong owner code and too many wrong codes get their own copy on every card, not only on payouts.
  String _errorText(Object e) {
    final t = context.t;
    if (e is! ApiException) return t.error(e);
    if (e.code == 'totp_invalid') return t.cOwnerCodeWrong;
    if (e.status == 429) return t.cOwnerCodeLocked;
    return switch (e.status) {
      400 => setupMessage(t, e.code),
      409 when e.code == 'card_locked' => t.cSetupLocked,
      403 => t.cSetupOwnerOnly,
      _ => t.error(e),
    };
  }

  String? get _savedMode {
    if (widget.card['source'] != 'console') return null;
    for (final f in _fields) {
      if (f['name'] == 'mode') return f['preview'] as String?;
    }
    return null;
  }

  /// Toss payouts move money: the server refuses a change without the owner's code. Saving also counts a
  /// typed-in toss mode; clearing counts only the saved one. Null means the owner cancelled the prompt.
  Future<Map<String, String>?> _ownerHeaders({required bool clearing}) async {
    // An open unlock window already carries the owner's code; the server ignores a second one.
    if (widget.card['unlock_expires_at'] != null) return const {};
    // The first save of a payment gateway card adds a place checkout money goes, so it needs the owner too.
    final typedToss = !clearing && _text['mode']?.text.trim() == 'toss';
    final needsCode = _gatewayCard
        ? !clearing && !_locked
        : _code == 'payout' && (typedToss || _savedMode == 'toss');
    if (!needsCode) {
      return const {};
    }
    final code = await askOwnerCode(context);
    return code == null ? null : {ownerTotpHeader: code};
  }

  Future<void> _test() async {
    final r = await context.api.post('/admin/setup/$_code/test') as Json;
    if (!mounted) return;
    setState(() {
      _result = r;
      _lastPass = r['last_test'] as Json?;
    });
  }

  /// Stored values leave the boxes once saved: from here on only the server's masked preview is shown.
  void _clearTyped() {
    setState(() {
      for (final c in _text.values) {
        c.clear();
      }
      for (final m in _map.values) {
        for (final c in m.values) {
          c.clear();
        }
      }
      _revealed.clear();
      _clearedSlots.clear();
      // The save clears the server-side test results, so the old ones must not show.
      _result = null;
      _lastPass = null;
    });
  }

  /// The first typed value that breaks its format, as a server code. Checked before any request is sent.
  String? _formatProblem() {
    for (final e in _text.entries) {
      final bad = formatError(_code, e.key, e.value.text);
      if (bad != null) return bad;
    }
    for (final e in _map.entries) {
      for (final m in e.value.values) {
        final bad = formatError(_code, e.key, m.text);
        if (bad != null) return bad;
      }
    }
    return null;
  }

  String? _errorFor(String field, String value) {
    final code = formatError(_code, field, value);
    return code == null ? null : setupMessage(context.t, code);
  }

  Future<void> _save() => _run(() async {
    final bad = _formatProblem();
    if (bad != null) {
      if (mounted) showToast(context, setupMessage(context.t, bad));
      return;
    }
    final headers = await _ownerHeaders(clearing: false);
    if (headers == null || !mounted) return;
    await context.api.put('/admin/setup/$_code', _body(), headers);
    if (!mounted) return;
    _clearTyped();
    showToast(context, context.t.cSetupSaved);
    widget.onChanged();
  });

  Future<void> _unlock() => _run(() async {
    final code = await askOwnerCode(context);
    if (code == null || !mounted) return;
    await context.api.post('/admin/setup/$_code/unlock', null, null, {
      ownerTotpHeader: code,
    });
    widget.onChanged();
  });

  Future<void> _lockNow() => _run(() async {
    await context.api.post('/admin/setup/$_code/lock');
    widget.onChanged();
  });

  Future<void> _clear() => _run(() async {
    final headers = await _ownerHeaders(clearing: true);
    if (headers == null || !mounted) return;
    await context.api.delete('/admin/setup/$_code', null, headers);
    widget.onChanged();
  });

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final configured = widget.card['configured'] == true;
    // Test or live from the saved Toss key prefix (server-side); null when there is none.
    final mode = widget.card['mode'] as String?;
    final ok = _result?['ok'] == true;
    final passKept = _result?['ok'] == false && _lastPass?['ok'] == true;
    final status = !configured
        ? Tag(label: t.cSetupNotConnected, hue: StatusHues.consultation)
        : widget.card['source'] == 'aws_role'
        ? Tag(label: t.cSetupAwsAuto, hue: StatusHues.assigned)
        : widget.card['source'] == 'env'
        ? Tag(label: t.cSetupFromEnv, hue: StatusHues.assigned)
        : _result == null
        ? Tag(label: t.cSetupSaved, hue: StatusHues.confirmed)
        : Tag(
            label: setupMessage(t, _result!['code'] as String),
            hue: ok ? StatusHues.assigned : StatusHues.cancelled,
          );
    return AppCard(
      padding: EdgeInsets.zero,
      child: Theme(
        data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
        child: ExpansionTile(
          expandedCrossAxisAlignment: CrossAxisAlignment.stretch,
          initiallyExpanded: !configured,
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
            configured
                ? Icons.check_circle_rounded
                : Icons.radio_button_unchecked_rounded,
            color: configured ? context.primary : null,
          ),
          title: KeepAllText(_guide.title(t), style: context.text.titleMedium),
          subtitle: Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Wrap(
              spacing: Space.xs,
              runSpacing: Space.xs,
              children: [
                status,
                if (passKept)
                  Tag(
                    label: t.cSetupLastPassKept,
                    hue: StatusHues.consultation,
                  ),
                if (widget.card['locked'] == true)
                  Tag(label: t.cSetupLockedBadge, hue: StatusHues.cancelled),
                if (widget.card['unlock_expires_at'] != null)
                  Tag(label: _unlockBadge(t), hue: StatusHues.assigned),
                if (mode != null)
                  Tag(
                    label: mode == 'live' ? t.cSetupModeLive : t.cSetupModeTest,
                    hue: mode == 'live'
                        ? StatusHues.assigned
                        : StatusHues.consultation,
                  ),
              ],
            ),
          ),
          children: [
            _Steps(guide: _guide),
            if (_code == 'portone' && widget.webhookUrl != null)
              CopyRow(label: t.cSetupWebhook, value: widget.webhookUrl!),
            if (_code == 'payout' && widget.tossWebhookUrl != null)
              CopyRow(
                label: t.cSetupTossWebhook,
                value: widget.tossWebhookUrl!,
              ),
            if (_code == 'tosspayments' &&
                widget.tossPaymentsWebhookUrl != null)
              CopyRow(
                label: t.cSetupTossWebhook,
                value: widget.tossPaymentsWebhookUrl!,
              ),
            const SizedBox(height: Space.md),
            for (final f in _fields) _field(context, f),
            const SizedBox(height: Space.md),
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
                // The server tests the saved values and takes none, so the owner runs it on a locked card
                // with no unlock. Anyone else gets a refusal there, so the server hides it (`can_test`).
                if (configured && widget.card['can_test'] != false)
                  OutlinedButton.icon(
                    onPressed: _busy ? null : () => _run(_test),
                    icon: const Icon(Icons.wifi_tethering_rounded),
                    label: KeepAllText(
                      _gatewayCard ? t.cSetupTestApiKey : t.cSetupTest,
                    ),
                  ),
                if (widget.card['locked'] == true &&
                    widget.card['source'] == 'console')
                  OutlinedButton.icon(
                    onPressed: _busy ? null : _unlock,
                    icon: const Icon(Icons.lock_open_rounded),
                    label: KeepAllText(t.cSetupUnlock),
                  ),
                if (widget.card['unlock_expires_at'] != null)
                  OutlinedButton.icon(
                    onPressed: _busy ? null : _lockNow,
                    icon: const Icon(Icons.lock_rounded),
                    label: KeepAllText(t.cSetupLockNow),
                  ),
                if (widget.card['source'] == 'console')
                  TextButton(
                    onPressed: _busy || _locked ? null : _clear,
                    child: KeepAllText(t.cSetupClear),
                  ),
                if (_busy)
                  const Padding(
                    padding: EdgeInsets.all(8),
                    child: SizedBox.square(
                      dimension: 20,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _revealToggle(String name) {
    final t = context.t;
    final shown = _revealed.contains(name);
    return IconButton(
      tooltip: shown ? t.cSetupHide : t.cSetupShow,
      icon: Icon(
        shown ? Icons.visibility_off_rounded : Icons.visibility_rounded,
      ),
      onPressed: () {
        setState(() => shown ? _revealed.remove(name) : _revealed.add(name));
      },
    );
  }

  Widget _slotField(
    BuildContext context,
    String field,
    MapEntry<String, TextEditingController> e,
    bool saved,
  ) {
    final t = context.t;
    // PortOne channel keys say where each key comes from; other maps (Solapi templates) use their field guide.
    final hint = field == 'channel_keys'
        ? _slotHint(t, e.key)
        : _guide.fields[field]?.hint?.of(context.lang);
    final cleared = _clearedSlots.contains(e.key);
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.xs),
      child: TextField(
        controller: e.value,
        enabled: !_locked,
        onChanged: (_) => setState(() {}),
        decoration: InputDecoration(
          labelText: slotLabels[e.key]?.of(context.lang) ?? e.key,
          helperText: !saved
              ? hint
              : cleared
              ? t.cSetupSlotCleared
              : [t.cSetupKeepValue, ?hint].join(' · '),
          isDense: true,
          errorText: _errorFor(field, e.value.text),
          // A saved slot can be removed alone (e.g. to move that method to Toss Payments).
          suffixIcon: saved && !_locked
              ? IconButton(
                  tooltip: cleared ? t.cSetupUndoClearSlot : t.cSetupClearSlot,
                  icon: Icon(
                    cleared ? Icons.undo_rounded : Icons.delete_outline_rounded,
                  ),
                  onPressed: () => setState(
                    () => cleared
                        ? _clearedSlots.remove(e.key)
                        : _clearedSlots.add(e.key),
                  ),
                )
              : null,
        ),
      ),
    );
  }

  Widget _field(BuildContext context, Json f) {
    final t = context.t;
    final name = f['name'] as String;
    final kind = f['kind'] as String;
    final g = _guide.fields[name];
    final label =
        '${localFields['$_code.$name']?.label(t) ?? g?.label.of(context.lang) ?? name}${f['required'] == true ? '' : ' (${t.cSetupOptional})'}';
    final saved = f['set'] == true;
    if (kind == 'map') {
      final entries = _map[name]!.entries;
      final savedSlots = f['preview'] is Map
          ? (f['preview'] as Map).keys.cast<String>().toSet()
          : const <String>{};
      return Padding(
        padding: const EdgeInsets.only(bottom: Space.sm),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            KeepAllText(label, style: context.text.labelLarge),
            const SizedBox(height: Space.xs),
            for (final e in entries)
              if (!otherSlotKeys.contains(e.key))
                _slotField(context, name, e, savedSlots.contains(e.key)),
            if (entries.any((e) => otherSlotKeys.contains(e.key))) ...[
              const SizedBox(height: Space.xs),
              KeepAllText(t.cSetupOtherMethods, style: context.text.labelMedium),
              for (final e in entries)
                if (otherSlotKeys.contains(e.key))
                  _slotField(context, name, e, savedSlots.contains(e.key)),
            ],
          ],
        ),
      );
    }
    final preview = f['preview'];
    if (kind == 'toggle') {
      final hint = g?.hint?.of(context.lang);
      return SwitchListTile(
        contentPadding: EdgeInsets.zero,
        title: KeepAllText(label),
        subtitle: hint == null ? null : KeepAllText(hint),
        value: _toggles[name] ?? preview == true,
        // A locked card cannot be saved, so its switches do not move either.
        onChanged: _locked ? null : (on) => setState(() => _toggles[name] = on),
      );
    }
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: TextField(
        controller: _text[name],
        enabled: !_locked,
        onChanged: (_) => setState(() {}),
        obscureText: kind == 'secret' && !_revealed.contains(name),
        maxLines: kind == 'file' ? 6 : 1,
        minLines: 1,
        autocorrect: false,
        enableSuggestions: false,
        decoration: InputDecoration(
          labelText: label,
          errorText: _errorFor(name, _text[name]!.text),
          suffixIcon: kind == 'secret' ? _revealToggle(name) : null,
          helperText: saved
              ? '${t.cSetupKeepValue} · ${preview ?? ''}'
              : localFields['$_code.$name']?.hint(t) ??
                    g?.hint?.of(context.lang),
        ),
      ),
    );
  }
}

/// Where each PortOne channel key comes from. Billing and identity have keys of their own.
String _slotHint(T t, String slot) => switch (slot) {
  'card' => t.cSlotHintCard,
  'card_billing' => t.cSlotHintBilling,
  'identity' => t.cSlotHintIdentity,
  'cash_receipt' => t.cSlotHintCashReceipt,
  _ => t.cSlotHintMethod,
};

class _Steps extends StatelessWidget {
  const _Steps({required this.guide});
  final Guide guide;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: KeepAllText(t.cSetupGuide, style: context.text.labelLarge),
            ),
            TextButton.icon(
              onPressed: () => launchUrl(
                Uri.parse(guide.site),
                mode: LaunchMode.externalApplication,
              ),
              icon: const Icon(Icons.open_in_new_rounded, size: 18),
              label: KeepAllText(t.cSetupOpenSite),
            ),
          ],
        ),
        if (guide.devHelp)
          Padding(
            padding: const EdgeInsets.only(bottom: Space.xs),
            child: Chip(
              avatar: const Icon(Icons.engineering_rounded, size: 18),
              label: KeepAllText(
                const Tr(
                  '개발자 첫 도움 필요',
                  'Needs a developer the first time',
                ).of(context.lang),
              ),
            ),
          ),
        for (final (i, step) in guide.steps.indexed)
          Padding(
            padding: const EdgeInsets.only(bottom: Space.xs),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                CircleAvatar(
                  radius: 11,
                  child: Text('${i + 1}', style: context.text.labelSmall),
                ),
                const SizedBox(width: Space.sm),
                Expanded(
                  child: KeepAllText(
                    step.of(context.lang),
                    style: context.text.bodyMedium,
                  ),
                ),
              ],
            ),
          ),
        for (final shot in guide.screenshots)
          Container(
            height: 120,
            alignment: Alignment.center,
            margin: const EdgeInsets.only(bottom: Space.sm),
            decoration: BoxDecoration(
              border: Border.all(color: Theme.of(context).dividerColor),
              borderRadius: BorderRadius.circular(8),
            ),
            child: KeepAllText(shot.of(context.lang), style: context.text.bodySmall),
          ),
      ],
    );
  }
}

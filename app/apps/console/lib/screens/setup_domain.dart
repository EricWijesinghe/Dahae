// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:convert';

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';
import 'package:url_launcher/url_launcher.dart';

import '../host_job.dart';
import '../state.dart';
import '../widgets/owner_totp_dialog.dart';
import 'setup_domain_checklist.dart';

/// The server agent's domain rule (`DOMAIN_RE` in deploy/agent.sh), so a typo fails before a job is queued.
final domainPattern = RegExp(
  r'^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+([a-z]{2,63}|xn--[a-z0-9-]{1,59})$',
);

/// How far a domain move got. [reason] is the agent's last step text when it stopped.
enum MovePhase { idle, running, awaiting, done, rolledBack, rejected, failed }

/// Domain settings: the current address and old ones still answering, a checked move (DNS check, owner
/// code, server approval code) and, after a move, the outside-service checklist.
class DomainCard extends StatelessWidget {
  const DomainCard({super.key, required this.business});

  /// The business profile, for the minimum app versions the retire dialog shows.
  final Json business;

  @override
  Widget build(BuildContext context) => AsyncView<Json>(
    load: () async => await context.api.get('/admin/domains') as Json,
    builder: (context, data, reload) =>
        _DomainBody(data: data, business: business, reload: reload),
  );
}

class _DomainBody extends StatefulWidget {
  const _DomainBody({
    required this.data,
    required this.business,
    required this.reload,
  });
  final Json data;
  final Json business;
  final Future<void> Function() reload;

  @override
  State<_DomainBody> createState() => _DomainBodyState();
}

class _DomainBodyState extends State<_DomainBody> {
  final _domain = TextEditingController();
  bool _editing = false;
  bool _invalid = false;
  bool _checking = false;
  Json? _dns;
  MovePhase _phase = MovePhase.idle;
  String? _reason;
  String? _jobId;
  String? _moved;

  String get _current => widget.data['current'] as String? ?? '';

  @override
  void dispose() {
    _domain.dispose();
    super.dispose();
  }

  String? _typed() {
    final d = _domain.text.trim().toLowerCase();
    return domainPattern.hasMatch(d) && d != _current ? d : null;
  }

  Future<void> _check() async {
    final d = _typed();
    setState(() {
      _invalid = d == null;
      _dns = null;
    });
    if (d == null) return;
    final api = context.api, t = context.t;
    setState(() => _checking = true);
    try {
      final job = await queueHostJob(api, 'domain-check', d);
      final end = await waitHostJob(api, job['id'] as String);
      final out = end['state'] == 'ok'
          ? await hostJobOutput(api, job['id'] as String)
          : null;
      final parsed = out == null ? null : jsonDecode(out);
      if (mounted) setState(() => _dns = parsed is Json ? parsed : null);
      if (parsed is! Json && mounted) {
        showToast(
          context,
          t.cRbDomainFailed(jobReason(end, t) ?? t.cRbDomainNoReason),
        );
      }
    } catch (e) {
      if (mounted) showToast(context, t.error(e));
    } finally {
      if (mounted) setState(() => _checking = false);
    }
  }

  Future<void> _move() async {
    // What the owner typed, not what the API echoed back: the check result only has to agree with it.
    final d = _typed();
    if (d == null || _dns?['domain'] != d) return;
    final api = context.api;
    final job = await runOwnerAction(
      context,
      (h) => queueHostJob(api, 'domain-set', d, h),
      done: context.t.cRbDomainRunning,
    );
    if (job is! Json || !mounted) return;
    setState(() {
      _jobId = job['id'] as String;
      _moved = d;
      _phase = MovePhase.running;
    });
    await _follow();
  }

  /// Waits for the job; on the agent's hold, asks for the server approval code and sends it.
  Future<void> _follow() async {
    final id = _jobId;
    if (id == null) return;
    final api = context.api;
    try {
      var job = await waitHostJob(api, id, until: {'awaiting_approval'});
      while (job['state'] == 'awaiting_approval') {
        if (!mounted) return;
        setState(() => _phase = MovePhase.awaiting);
        final t = context.t;
        final summary = await _serverSummary(api, job);
        if (!mounted) return;
        if (summary == null) {
          showToast(context, t.cRbApprovalNoSummary);
          return;
        }
        final code = await askOwnerTotp(
          context,
          title: t.cRbApprovalTitle,
          body: '${t.cRbApprovalBody}\n\n$summary',
        );
        if (code == null || !mounted) return;
        setState(() => _phase = MovePhase.running);
        await api.post('/admin/host/jobs/$id/approve', {'code': code});
        // The job stays on hold until the agent reads the code, so wait for it to start. Still on hold after
        // that means a wrong code: ask again, until the agent cancels the job.
        job = await waitHostJob(
          api,
          id,
          until: {'running'},
          limit: const Duration(seconds: 60),
        );
      }
      // A move waits up to 10 minutes for the new address, then may roll back (agent run limit 30 minutes).
      if (job['state'] == 'running') {
        job = await waitHostJob(api, id, limit: const Duration(minutes: 30));
      }
      if (mounted) _finish(job);
    } catch (e) {
      if (!mounted) return;
      setState(() => _phase = MovePhase.awaiting);
      showToast(context, context.t.error(e));
    }
  }

  /// The root agent's own summary of the held job, which the API cannot write (Caddy serves it from the agent's
  /// public folder). Null unless it says the domain moves to what the owner typed: then do not approve.
  Future<String?> _serverSummary(ApiClient api, Json job) async {
    final token = job['progress_token'], moved = _moved;
    if (token is! String || moved == null) return null;
    // The agent writes the hold state a moment before the summary.
    for (var i = 0; i < 5; i++) {
      final p = await api.getServerFile('/ops-progress/$token');
      final s = p is Map ? p['summary'] : null;
      if (p is Map && p['state'] == 'awaiting_approval' && s is String) {
        return s.contains('도메인을 $moved 로') ? s : null;
      }
      await Future<void>.delayed(const Duration(seconds: 1));
    }
    return null;
  }

  void _finish(Json job) {
    setState(() {
      _reason = jobReason(job, context.t);
      _phase = switch (job['state']) {
        'ok' => MovePhase.done,
        'rolled_back' => MovePhase.rolledBack,
        'rejected' => MovePhase.rejected,
        _ => MovePhase.failed,
      };
      if (_phase == MovePhase.done) _editing = false;
    });
    if (_phase == MovePhase.done) widget.reload();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final aliases = (widget.data['aliases'] as List? ?? const []).cast<Json>();
    final checklist = (widget.data['checklist'] as List? ?? const [])
        .cast<Json>();
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(t.cRbDomainTitle, style: context.text.titleSmall),
          const SizedBox(height: Space.xs),
          Text(t.cRbDomainCurrent(_current), style: context.text.bodyMedium),
          if (aliases.isNotEmpty) ...[
            const SizedBox(height: Space.sm),
            Text(t.cRbDomainAliases, style: context.text.labelLarge),
            for (final a in aliases)
              AliasRow(
                host: a['host'] as String,
                business: widget.business,
                onRetired: widget.reload,
              ),
          ],
          const SizedBox(height: Space.sm),
          ..._moveSection(t),
          // The server always sends the list; it only matters after a move.
          if (checklist.isNotEmpty && aliases.isNotEmpty) ...[
            const SizedBox(height: Space.md),
            DomainChecklist(items: checklist),
          ],
        ],
      ),
    );
  }

  List<Widget> _moveSection(T t) {
    final status = _status(t);
    if (!_editing) {
      return [
        ?status,
        OutlinedButton(
          onPressed: () => setState(() => _editing = true),
          child: Text(t.cRbDomainChange),
        ),
      ];
    }
    final dns = _dns;
    final busy = _phase == MovePhase.running || _phase == MovePhase.awaiting;
    return [
      TextField(
        controller: _domain,
        enabled: !busy,
        keyboardType: TextInputType.url,
        autocorrect: false,
        decoration: InputDecoration(
          labelText: t.cRbDomainNew,
          errorText: _invalid ? t.cRbDomainInvalid : null,
        ),
        onChanged: (_) => setState(() => _dns = null),
      ),
      const SizedBox(height: Space.sm),
      OutlinedButton(
        onPressed: _checking || busy ? null : _check,
        child: Text(_checking ? t.cRbDomainChecking : t.cRbDomainCheck),
      ),
      if (dns != null) ...[
        const SizedBox(height: Space.sm),
        DnsResult(result: dns),
      ],
      if (dns?['match'] == true) ...[
        const SizedBox(height: Space.md),
        Text(t.cRbDomainWhatTitle, style: context.text.labelLarge),
        Text(t.cRbDomainWhatList, style: context.text.bodySmall),
        const SizedBox(height: Space.sm),
        FilledButton(
          style: FilledButton.styleFrom(minimumSize: const Size(0, 48)),
          onPressed: busy ? null : _move,
          child: Text(t.cRbDomainApply),
        ),
      ],
      ?status,
    ];
  }

  Widget? _status(T t) {
    final why = _reason ?? t.cRbDomainNoReason;
    final text = switch (_phase) {
      MovePhase.idle => null,
      MovePhase.running => t.cRbDomainRunning,
      MovePhase.awaiting => t.cRbApprovalTitle,
      MovePhase.done => t.cRbDomainDone(_moved ?? ''),
      MovePhase.rolledBack => t.cRbDomainRolledBack(why),
      // An agent from before domain moves refuses the kind without saying why.
      MovePhase.rejected =>
        _reason == null ? t.cRbDomainOldAgent : t.cRbDomainRejected(why),
      MovePhase.failed => t.cRbDomainFailed(why),
    };
    if (text == null) return null;
    final bad = {
      MovePhase.rolledBack,
      MovePhase.rejected,
      MovePhase.failed,
    }.contains(_phase);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Space.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            text,
            style: context.text.bodyMedium?.copyWith(
              color: bad ? context.colors.danger : null,
            ),
          ),
          if (_phase == MovePhase.awaiting)
            TextButton(onPressed: _follow, child: Text(t.cRbApprovalEnter)),
          if (_phase == MovePhase.done && _moved != null)
            TextButton(
              onPressed: () => launchUrl(Uri.parse('https://$_moved/admin/')),
              child: Text(t.cRbDomainOpenNew),
            ),
        ],
      ),
    );
  }
}

/// The `domain-check` answer in plain words: where A should point, and whether AAAA is in the way.
class DnsResult extends StatelessWidget {
  const DnsResult({super.key, required this.result});
  final Json result;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final ip = result['server_ip'] as String? ?? '';
    final a = (result['a'] as List? ?? const []).join(', ');
    final aOk = (result['a'] as List? ?? const []).contains(ip);
    final ok = result['match'] == true;
    final color = ok ? context.colors.fresh : context.colors.danger;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          aOk
              ? t.cRbDomainDnsOk(ip)
              : t.cRbDomainDnsBad(ip, a.isEmpty ? t.cRbDomainDnsNone : a),
          style: context.text.bodyMedium?.copyWith(
            color: aOk ? color : context.colors.danger,
          ),
        ),
        if (result['aaaa_ok'] == false)
          Text(
            t.cRbDomainDnsAaaa,
            style: context.text.bodyMedium?.copyWith(
              color: context.colors.danger,
            ),
          ),
      ],
    );
  }
}

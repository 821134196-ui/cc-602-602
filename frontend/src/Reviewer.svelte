<script>
  import { onMount } from 'svelte'
  import { api, downloadFile } from './api.js'
  import { REC_TEXT, fmtTime } from './text.js'

  let assignments = []
  let error = ''
  let notice = ''
  // 每个任务的评阅表单：{ [assignmentId]: {comment, recommendation, score} }
  let forms = {}

  async function load() {
    assignments = await api('/api/reviewer/assignments')
    for (const a of assignments) {
      if (!forms[a.assignment_id]) {
        forms[a.assignment_id] = a.my_review
          ? { comment: a.my_review.comment, recommendation: a.my_review.recommendation, score: a.my_review.score }
          : { comment: '', recommendation: 'pass', score: '' }
      }
    }
  }

  onMount(async () => {
    try { await load() } catch (e) { error = e.message }
  })

  async function submit(a) {
    error = ''; notice = ''
    const f = forms[a.assignment_id]
    try {
      await api(`/api/reviewer/assignments/${a.assignment_id}/review`, {
        method: 'POST',
        body: {
          comment: f.comment,
          recommendation: f.recommendation,
          score: f.score === '' || f.score == null ? null : Number(f.score)
        }
      })
      notice = `稿件 ${a.thesis_code} 的评阅意见已提交`
      await load()
    } catch (e) { error = e.message }
  }

  async function download(a) {
    error = ''
    try { await downloadFile(a.version_id, a.filename) } catch (e) { error = e.message }
  }
</script>

{#if error}<div class="error">{error}</div>{/if}
{#if notice}<div class="ok">{notice}</div>{/if}

{#each assignments as a}
  <div class="card">
    <h2>稿件 {a.thesis_code} <span class="muted">（匿名编号）</span></h2>
    <div class="muted">题目：《{a.title}》 ｜ 分配时间：{fmtTime(a.assigned_at)}</div>
    <div class="row" style="margin:10px 0">
      <span>送审稿件：{a.filename}</span>
      <button class="ghost" on:click={() => download(a)}>下载稿件</button>
    </div>

    {#if a.submitted}
      <h3>已提交的评阅意见</h3>
      <div class="review-box">
        <div>
          <span class="badge">{REC_TEXT[a.my_review.recommendation] || a.my_review.recommendation}</span>
          {#if a.my_review.score != null}<span class="badge">{a.my_review.score} 分</span>{/if}
        </div>
        <div style="margin-top:6px">{a.my_review.comment}</div>
        <div class="muted">提交时间：{fmtTime(a.my_review.submitted_at)}（意见开放前可修改后重新提交）</div>
      </div>
    {/if}

    <h3>{a.submitted ? '修改评阅意见' : '填写评阅意见'}</h3>
    <div class="row">
      <select bind:value={forms[a.assignment_id].recommendation}>
        <option value="pass">通过</option>
        <option value="minor_revision">小修后通过</option>
        <option value="major_revision">大修后重审</option>
        <option value="reject">不通过</option>
      </select>
      <input style="width:100px" type="number" min="0" max="100" placeholder="分数 0-100"
        bind:value={forms[a.assignment_id].score} />
    </div>
    <textarea placeholder="评阅意见" bind:value={forms[a.assignment_id].comment}></textarea>
    <div style="margin-top:8px">
      <button on:click={() => submit(a)}>提交评阅意见</button>
    </div>
  </div>
{/each}

{#if assignments.length === 0}
  <div class="card muted">暂无分配给您的评阅任务。</div>
{/if}

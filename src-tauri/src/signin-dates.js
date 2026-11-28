(() => {
  if (location.origin !== 'https://www.natpierce.cn' || location.pathname !== '/pc/sign/index.html' || window.top !== window) return null
  const username = sessionStorage.getItem('llremote-signin-account')
  if (!username) return null

  const read = text => {
    const date = '(\\d{4}-\\d{1,2}-\\d{1,2}\\s+\\d{1,2}:\\d{2}(?::\\d{2})?)'
    const find = label => text.match(new RegExp(`${label}\\s*[：:]?\\s*${date}`))?.[1].replace(/\s+/g, ' ') ?? null
    return { username, expiresAt: find('服务到期时间'), availableAt: find('下次可签到时间') }
  }

  if (window.jQuery && !window.llRemoteSigninWatching) {
    window.llRemoteSigninWatching = true
    window.jQuery(document).ajaxSuccess(async (_, response, request) => {
      if (new URL(request.url, location.href).href !== `${location.origin}/pc/sign/qiandao.html` || String(response.responseJSON?.code) !== '200') return
      try {
        const page = await fetch(location.pathname, { cache: 'no-store', signal: AbortSignal.timeout(5000) })
        if (!page.ok || new URL(page.url).pathname !== location.pathname) return
        const body = new DOMParser().parseFromString(await page.text(), 'text/html').body
        body.querySelectorAll('script, style').forEach(node => node.remove())
        const dates = read(body.textContent)
        if (dates.expiresAt || dates.availableAt) window.llRemoteSigninUpdatedDates = dates
      } catch {
        // The next page load can supply the updated dates.
      }
    })
  }

  return window.llRemoteSigninUpdatedDates ?? read(document.body?.innerText ?? '')
})()

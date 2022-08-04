export default {
    projectLink: 'https://github.com/dghilardi/hailstorm', // GitHub link in the navbar
    docsRepositoryBase: 'https://github.com/shuding/nextra/blob/master', // base URL for the docs repository
    titleSuffix: ' – Nextra',
    nextLinks: true,
    prevLinks: true,
    search: true,
    customSearch: null, // customizable, you can use algolia for example
    darkMode: true,
    footer: true,
    footerText: `MIT ${new Date().getFullYear()} © COMELIT S.p.A.`,
    footerEditLink: `Edit this page on GitHub`,
    logo: (
      <>
      <span className="mr-2 font-extrabold hidden md:inline">Hailstorm</span>
      <span className="text-gray-600 font-normal hidden md:inline">
        Stress test library
      </span>
      </>
    ),
    head: (
      <>
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <meta name="description" content="Nextra: the next docs builder" />
        <meta name="og:title" content="Nextra: the next docs builder" />
      </>
    ),
  }
import { defineCollection } from "astro:content";
import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";

const documentId = ({ entry }: { entry: string }) =>
  entry.replace(/\.(md|mdx)$/, "").replace(/\/index$/, "");

export const collections = {
  docs: defineCollection({ loader: docsLoader({ generateId: documentId }), schema: docsSchema() }),
};

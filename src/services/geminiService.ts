import { GoogleGenAI, Type } from "@google/genai";

const ai = new GoogleGenAI({ apiKey: process.env.GEMINI_API_KEY || "" });

export interface AgentPlan {
  steps: {
    action: string;
    description: string;
    target?: string;
  }[];
  summary: string;
}

export async function interpretIntent(intent: string): Promise<AgentPlan> {
  const response = await ai.models.generateContent({
    model: "gemini-3-flash-preview",
    contents: `You are Sextant, an agentic browser. Interpret the following user intent and generate a step-by-step plan to execute it on the web.
    User Intent: "${intent}"
    
    Return a JSON object with:
    - steps: Array of { action: string, description: string, target?: string }
    - summary: A brief summary of the plan.`,
    config: {
      responseMimeType: "application/json",
      responseSchema: {
        type: Type.OBJECT,
        properties: {
          steps: {
            type: Type.ARRAY,
            items: {
              type: Type.OBJECT,
              properties: {
                action: { type: Type.STRING },
                description: { type: Type.STRING },
                target: { type: Type.STRING },
              },
              required: ["action", "description"],
            },
          },
          summary: { type: Type.STRING },
        },
        required: ["steps", "summary"],
      },
    },
  });

  try {
    return JSON.parse(response.text || "{}");
  } catch (e) {
    console.error("Failed to parse agent plan", e);
    return { steps: [], summary: "Failed to interpret intent." };
  }
}

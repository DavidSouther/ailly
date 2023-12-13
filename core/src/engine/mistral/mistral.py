from ctransformers import AutoModelForCausalLM
import sys
import os


class Mistral:
    def __init__(self, path="./mistral-7b-instruct-v0.1.Q4_K_M.gguf"):
        self.llm = AutoModelForCausalLM.from_pretrained(
            model_path_or_repo_id=path, model_type="mistral", gpu_layers=50
        )

    def infer(self, prompt):
        response = self.llm(prompt)
        return response


if __name__ == "__main__":
    tty = os.open("/dev/tty", os.O_RDWR)
    print("Running mistral.py\n", file=tty)
    mistral = Mistral()
    prompt = "\n".join(sys.argv[1:])
    response = mistral.infer(prompt)
    print(response)

#include "ConstantBuffer.hlsl"
#include "VertexInput.hlsl"
#include "Utils.hlsl"

struct VS_OUTPUT
{
	float4 Pos : SV_POSITION;
	float4 Colour : COLOR;
};

VS_OUTPUT vs_main( VS_INPUT_Wireframe In )
{
	VS_OUTPUT Out;

	float4 cameraPos = mul(cameraFromObjectMatrix, float4 ( In.Pos, 1.0f ) );
	float4 projectionPos = mul(projectionFromCameraMatrix, cameraPos);
	Out.Pos = projectionPos;
	Out.Colour = In.Colour;
	
	return Out;
}

struct PS_OUTPUT
{
	float4 Colour : COLOR0;
};

PS_OUTPUT ps_main( VS_OUTPUT In ) : SV_TARGET
{
	PS_OUTPUT result;

	result.Colour = In.Colour;

	return result;
}



